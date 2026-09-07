import { derived, get, writable } from 'svelte/store';
import { decodeKepJsonFrames, encodeKepStreamFrame, encodeOscPacket } from './kep.js';
const emptyWorld = () => ({ tick: 0, objects: [] });
export function createAdminClient(input) {
    const options = Object.freeze({ ...input });
    const fetcher = options.fetch ?? globalThis.fetch?.bind(globalThis);
    const connectionState = writable('idle');
    const worldSnapshot = writable(emptyWorld());
    const debugLogs = writable([]);
    const lastError = writable(null);
    const appActionCatalog = writable({ actions: [] });
    const webTransportState = writable(options.webTransportUrl ? 'closed' : 'disabled');
    const webTransportDetail = writable(options.webTransportUrl ?? null);
    const lastOscSendStatus = writable({ path: 'none', phase: 'idle', detail: null });
    const objectCount = derived(worldSnapshot, snapshot => snapshot.objects.length);
    let socket = null;
    let reconnectTimer = null;
    let transport = null;
    let transportReady = false;
    let stopped = true;
    let lifecycleEpoch = 0;
    const pendingRequests = new Set();
    function createSocket(url) {
        if (options.createWebSocket)
            return options.createWebSocket(url);
        return new WebSocket(url);
    }
    function start() {
        if (options.isBrowser === false || (options.isBrowser === undefined && typeof window === 'undefined'))
            return;
        if (!stopped)
            return;
        stopped = false;
        connect();
    }
    function connect() {
        if (stopped || socket?.readyState === 0 || socket?.readyState === 1)
            return;
        reconnectTimer = null;
        advanceEpoch();
        connectionState.set('connecting');
        const epoch = lifecycleEpoch;
        const next = createSocket(options.webSocketUrl);
        socket = next;
        next.addEventListener('open', () => {
            if (!isCurrent(epoch) || socket !== next)
                return;
            connectionState.set('open');
            lastError.set(null);
            void loadAppActions();
            connectWebTransport();
        });
        next.addEventListener('message', event => {
            if (!isCurrent(epoch) || socket !== next)
                return;
            try {
                applyServerEvent(JSON.parse(String(event.data)));
            }
            catch (error) {
                lastError.set(error instanceof Error ? error.message : String(error));
            }
        });
        next.addEventListener('close', () => {
            if (!isCurrent(epoch) || socket !== next)
                return;
            advanceEpoch();
            socket = null;
            connectionState.set('closed');
            resetTransport('closed', 'WebSocket closed');
            if (!stopped)
                reconnectTimer = setTimeout(connect, options.reconnectDelayMs ?? 1500);
        });
        next.addEventListener('error', () => {
            if (!isCurrent(epoch) || socket !== next)
                return;
            connectionState.set('error');
            lastError.set('WebSocket connection failed');
        });
    }
    function stop() {
        advanceEpoch();
        stopped = true;
        if (reconnectTimer)
            clearTimeout(reconnectTimer);
        reconnectTimer = null;
        const current = socket;
        socket = null;
        current?.close();
        resetTransport(options.webTransportUrl ? 'closed' : 'disabled', options.webTransportUrl ? 'stopped' : null);
        connectionState.set('idle');
    }
    function advanceEpoch() {
        lifecycleEpoch += 1;
        for (const controller of pendingRequests)
            controller.abort();
        pendingRequests.clear();
    }
    function isCurrent(epoch) {
        return lifecycleEpoch === epoch;
    }
    function resetTransport(state, detail) {
        const current = transport;
        transport = null;
        transportReady = false;
        current?.close();
        webTransportState.set(state);
        webTransportDetail.set(detail);
    }
    function sendOsc(payload) {
        if (transportReady && transport) {
            const epoch = lifecycleEpoch;
            const session = transport;
            const sendingSocket = socket;
            lastOscSendStatus.set({ path: 'webtransport', phase: 'pending', detail: 'opening stream' });
            void sendOverWebTransport(session, payload, epoch)
                .then(written => {
                if (!written && isCurrent(epoch) && transport === session)
                    sendOverWebSocket(payload, 'websocket-fallback', 'WebTransport pre-write failure', sendingSocket);
            })
                .catch((error) => {
                if (!isCurrent(epoch) || transport !== session)
                    return;
                const detail = error instanceof Error ? error.message : String(error);
                lastOscSendStatus.set({ path: 'webtransport', phase: 'failed', detail });
                lastError.set(detail);
            });
            return true;
        }
        return sendOverWebSocket(payload, 'websocket', webTransportReason());
    }
    function sendOverWebSocket(payload, path, detail, expectedSocket = socket) {
        if (!socket || socket !== expectedSocket || socket.readyState !== 1) {
            lastOscSendStatus.set({ path, phase: 'failed', detail: 'WebSocket is not connected' });
            lastError.set('WebSocket is not connected');
            return false;
        }
        socket.send(JSON.stringify(payload));
        lastOscSendStatus.set({ path, phase: path === 'websocket-fallback' ? 'fallback' : 'sent', detail });
        lastError.set(null);
        return true;
    }
    async function sendOverWebTransport(session, payload, epoch) {
        let stream;
        try {
            stream = await session.createBidirectionalStream();
        }
        catch {
            return false;
        }
        if (!isCurrent(epoch) || transport !== session)
            return true;
        const writer = stream.writable.getWriter();
        try {
            try {
                await writer.write(encodeKepStreamFrame(encodeOscPacket(payload), options.kepRoute ?? '/room/main'));
            }
            catch {
                return false;
            }
            if (!isCurrent(epoch) || transport !== session)
                return true;
            await writer.close();
            if (!isCurrent(epoch) || transport !== session)
                return true;
            const reader = stream.readable.getReader();
            const chunks = [];
            let length = 0;
            try {
                while (true) {
                    const next = await reader.read();
                    if (!isCurrent(epoch) || transport !== session)
                        return true;
                    if (next.done)
                        break;
                    chunks.push(next.value);
                    length += next.value.length;
                }
            }
            finally {
                reader.releaseLock();
            }
            const bytes = new Uint8Array(length);
            let offset = 0;
            for (const chunk of chunks) {
                bytes.set(chunk, offset);
                offset += chunk.length;
            }
            if (!isCurrent(epoch) || transport !== session)
                return true;
            for (const json of decodeKepJsonFrames(bytes))
                applyServerEvent(JSON.parse(json));
            lastOscSendStatus.set({ path: 'webtransport', phase: 'sent', detail: 'response applied' });
            lastError.set(null);
            return true;
        }
        finally {
            writer.releaseLock();
        }
    }
    function connectWebTransport() {
        if (!options.webTransportUrl || transport)
            return;
        try {
            const hash = parseCertificate(options.webTransportCertificateSha256);
            if (options.webTransportCertificateSha256 && !hash)
                return;
            const factory = options.createWebTransport ?? nativeWebTransport;
            transport = factory(options.webTransportUrl, hash ?? undefined);
            const session = transport;
            const epoch = lifecycleEpoch;
            webTransportState.set('connecting');
            webTransportDetail.set(options.webTransportUrl);
            session.ready
                .then(() => {
                if (!isCurrent(epoch) || transport !== session)
                    return;
                transportReady = true;
                webTransportState.set('ready');
            })
                .catch((error) => {
                if (!isCurrent(epoch) || transport !== session)
                    return;
                transport = null;
                transportReady = false;
                webTransportState.set('error');
                const detail = error instanceof Error ? error.message : String(error);
                webTransportDetail.set(detail);
                lastError.set(`WebTransport connection failed: ${detail}`);
            });
            session.closed
                .catch(() => null)
                .finally(() => {
                if (!isCurrent(epoch) || transport !== session)
                    return;
                transport = null;
                transportReady = false;
                webTransportState.set('closed');
                webTransportDetail.set('session closed');
            });
        }
        catch (error) {
            webTransportState.set('unsupported');
            webTransportDetail.set(error instanceof Error ? error.message : String(error));
        }
    }
    function nativeWebTransport(url, hash) {
        const Constructor = window.WebTransport;
        if (!Constructor)
            throw new Error('window.WebTransport is unavailable');
        return hash
            ? new Constructor(url, { serverCertificateHashes: [{ algorithm: 'sha-256', value: hash }] })
            : new Constructor(url);
    }
    function parseCertificate(value) {
        if (!value)
            return null;
        const hex = value.replace(/[^a-fA-F0-9]/g, '');
        if (hex.length !== 64) {
            webTransportState.set('error');
            webTransportDetail.set('certificate SHA-256 must be 64 hex chars');
            lastError.set('WebTransport certificate SHA-256 must be 64 hex chars');
            return null;
        }
        return Uint8Array.from({ length: 32 }, (_, index) => Number.parseInt(hex.slice(index * 2, index * 2 + 2), 16));
    }
    function webTransportReason() {
        if (!options.webTransportUrl)
            return 'WebTransport disabled';
        return `WebTransport ${get(webTransportState)}`;
    }
    async function request(path, init) {
        if (!fetcher)
            throw new Error('fetch is unavailable');
        return fetcher(`${options.apiUrl}${path}`, init);
    }
    async function lifecycleRequest(path, init, epoch) {
        const controller = new AbortController();
        pendingRequests.add(controller);
        try {
            return await request(path, { ...init, signal: controller.signal });
        }
        finally {
            pendingRequests.delete(controller);
        }
    }
    async function loadAppActions() {
        const epoch = lifecycleEpoch;
        try {
            const response = await lifecycleRequest('/app-actions', undefined, epoch);
            if (!isCurrent(epoch))
                return null;
            if (!response.ok)
                throw new Error(`App action catalog request failed: ${response.status}`);
            const catalog = (await response.json());
            if (!isCurrent(epoch))
                return null;
            appActionCatalog.set(catalog);
            return catalog;
        }
        catch (error) {
            if (!isCurrent(epoch))
                return null;
            lastError.set(error instanceof Error ? error.message : String(error));
            return null;
        }
    }
    async function runAppAction(actionId, inputs) {
        const epoch = lifecycleEpoch;
        lastError.set(null);
        lastOscSendStatus.set({ path: 'http', phase: 'pending', detail: actionId });
        try {
            const response = await lifecycleRequest(`/app-actions/${encodeURIComponent(actionId)}/run`, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ inputs })
            }, epoch);
            if (!isCurrent(epoch))
                return false;
            if (!response.ok) {
                const body = (await response.json().catch(() => null));
                if (!isCurrent(epoch))
                    return false;
                throw new Error(body?.error ?? `App action failed: ${response.status}`);
            }
            const result = (await response.json());
            if (!isCurrent(epoch))
                return false;
            worldSnapshot.set(result.snapshot);
            lastOscSendStatus.set({ path: 'http', phase: 'applied', detail: `${result.actionId}: ${result.osc.address}` });
            return true;
        }
        catch (error) {
            if (!isCurrent(epoch))
                return false;
            const detail = error instanceof Error ? error.message : String(error);
            lastOscSendStatus.set({ path: 'http', phase: 'failed', detail });
            lastError.set(detail);
            return false;
        }
    }
    const spawnObject = (kind, x, y, z) => runAppAction('spawn-object', {
        kind: { type: 'string', value: kind },
        x: { type: 'float', value: x },
        y: { type: 'float', value: y },
        z: { type: 'float', value: z }
    });
    const moveObject = (id, x, y, z) => runAppAction('move-object', {
        id: { type: 'string', value: id },
        x: { type: 'float', value: x },
        y: { type: 'float', value: y },
        z: { type: 'float', value: z }
    });
    const resetWorld = () => runAppAction('reset-world', {});
    function applyServerEvent(event) {
        if (event.type === 'state')
            worldSnapshot.set(event.snapshot);
        else if (event.type === 'log')
            debugLogs.update(items => [event.entry, ...items.filter(item => item.id !== event.entry.id)].slice(0, 500));
        else if (event.type === 'error')
            lastError.set(event.message);
        else if (event.type === 'connected')
            debugLogs.update(items => [
                {
                    id: Date.now(),
                    level: 'info',
                    message: `connected ${event.protocol}`,
                    oscAddress: null,
                    tick: event.tick
                },
                ...items
            ].slice(0, 500));
        else if (event.type === 'osc')
            debugLogs.update(items => [
                {
                    id: Date.now() + Math.random(),
                    level: 'info',
                    message: `backend -> admin ${event.address}`,
                    oscAddress: event.address,
                    tick: get(worldSnapshot).tick
                },
                ...items
            ].slice(0, 500));
    }
    const view = derived([
        connectionState,
        worldSnapshot,
        debugLogs,
        lastError,
        appActionCatalog,
        webTransportState,
        webTransportDetail,
        lastOscSendStatus,
        objectCount
    ], ([connectionState, worldSnapshot, debugLogs, lastError, appActionCatalog, webTransportState, webTransportDetail, lastOscSendStatus, objectCount]) => ({
        connectionState,
        worldSnapshot,
        debugLogs,
        lastError,
        appActionCatalog,
        webTransportState,
        webTransportDetail,
        lastOscSendStatus,
        objectCount
    }));
    return {
        subscribe: view.subscribe,
        options,
        apiBaseUrl: () => options.apiUrl,
        request,
        start,
        stop,
        sendOsc,
        loadAppActions,
        runAppAction,
        spawnObject,
        moveObject,
        resetWorld,
        applyServerEvent,
        connectionState,
        worldSnapshot,
        debugLogs,
        lastError,
        appActionCatalog,
        webTransportState,
        webTransportDetail,
        lastOscSendStatus,
        objectCount
    };
}
