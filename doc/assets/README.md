# README illustration sources

These eight illustrations were regenerated together for the framework/application
split documented in ADR 0007, using the built-in image generation tool. Their
English text was checked against the framework checkout at `e90d04b` and the
updated root README. Review them again when those contracts change.

All images use the same 1536 × 1024 layout and palette: navy headings, violet
runtime, teal content/adapters, blue clients, and amber developer tools. The
introduction image is the visual style reference for the other seven images.
The stable PNG filenames are referenced by the root README.

## Shared generation prompt

Use case: infographic-diagram. Asset: English GitHub README technical illustration for Kitu Logic Processor. Create a polished precise editorial technical diagram, 1536x1024 landscape. White background, navy #17243A headings and body, violet #6D5BD0 for Rust runtime, teal #008D91 for data/adapters, blue #3478D4 for clients, amber #D99025 for developer tools. Very pale tinted rounded panels, thin crisp outlines and connectors, flat vector-like line icons, no texture, no gradients, no 3D, no shadows. Consistent generous 60px outer margins, large top-left title, small uppercase eyebrow 'KITU / FRAMEWORK GUIDE', short subtitle, three or four spacious main panels, tiny footer 'Runtime • Applications • Presentation'. Modern clear sans-serif, high legibility at README width; use only supplied labels, no filler text, no invented capabilities, no code snippets. All arrows must have explicit single arrowheads matching stated direction; no overlapping connectors or text.

The following prompts record the initial generation. The reviewed correction
prompts at the end define the final Unity and communication images.

## Per-image prompts

### `introduction.png`

Title: 'Introduction'. Subtitle: 'Authoritative Rust logic, independent applications, shared tools'. Main diagram: large left blue panel 'Engine client' with three rows 'Input', 'Visuals / audio / UI', 'Asset lifetime'; center violet panel 'Application + Kitu Runtime' with three rows 'Rules and state', 'Fixed ticks', 'Ordered outputs'; right teal panel 'Content adapters' with rows 'Tanu / SQLite', 'Rhai', 'TSQ1'. Arrow from Engine client to Runtime labeled 'Typed input'; separate arrow from Runtime to Engine client labeled 'State and events'; arrow from Content adapters to Runtime labeled 'Validated content'. Lower amber wide panel 'Developer tools' with three compartments 'CLI / Shell', 'Web Admin', 'Replay verification', connected bidirectionally to Runtime labeled 'Host contracts'. Below title above main panels, small note 'Reference client: Unity'. Bottom concise note 'Applications own rules and activation policy'. No Bevy, no guarantee of cross-platform determinism, no automatic hot reload claims.

### `high-level-architecture.png`

Title: 'High-Level Architecture'. Subtitle: 'Reusable framework, independently versioned application'. Upper violet wide rounded container headed 'kitu-logic-processor', containing 4 evenly spaced cards 'Runtime + ECS', 'Data / script / timeline adapters', 'OSC-IR + transport + C ABI', 'Shell + shared Admin'. Lower blue wide rounded container headed 'kitu-unity-demo-game', containing 4 cards 'Rules + content', 'Server / embedded host', 'Unity client', 'Application Admin'. Between containers one strong UPWARD arrow from application container to framework container labeled 'Consumes framework interfaces'. Inside lower container show small directional arrows from Unity client to Server / embedded host labeled 'Input' and from host to client labeled 'Outputs', OR if cannot fit omit internal arrows completely. Bottom amber narrow wide bar labeled 'kitu-workspace' with sublabel 'Pins a tested combination of independent repositories'. No old apps/demo-game path, no Bevy, no claim that framework contains game rules. Prioritize spacious hierarchy and exact repository spelling. Reference image is STYLE ONLY; generate new composition and content.

### `backend-game-logic-rust.png`

Title: 'Backend Game Logic'. Subtitle: 'Application rules execute through the Kitu Runtime'. Main central violet horizontal sequence with 4 cards connected strictly left to right by arrows: 'Admit ordered input', 'Advance fixed tick', 'Run application + ECS', 'Publish ordered output'. Label above first two cards 'Transport input: receive N, apply N+1'. Beneath sequence, two equal wide panels: teal 'Application owns' with rows 'Rules and persistent state', 'Content activation', 'Validated script actions'; blue 'Host schedules' with rows 'Standalone server', 'Embedded native library', 'Same application construction'. Bottom amber strip 'Inspection and recording' with rows or inline labels 'Detached projections' and 'Committed input order'. Small note 'Replay requires matching code, content, and inputs'. No Bevy, no built-in physics/AI, no bit-for-bit cross-platform claim. Reference image is STYLE ONLY; generate new composition and content.

### `communication-layer-osc-osc-ir-messagepack.png`

Title: 'Communication Layer'. Subtitle: 'Typed OSC-IR across application boundaries'. Three equal large panels, left blue 'Engine client' with rows 'Typed input bundles', 'Ordered output batches'; center violet 'Application host' with rows 'Admission', 'Authoritative tick', 'Publication'; right amber 'CLI / Web Admin' with rows 'Inspection', 'Actions', 'Shell'. TWO visibly separate horizontal arrows between left and center: left to center labeled 'Input', center to left labeled 'Output'. TWO visibly separate horizontal arrows between right and center: right to center labeled 'Requests', center to right labeled 'Results'. Below, two wide teal cards: 'Embedded connection' with subtext 'Versioned C ABI' and 'Typed JSON / MessagePack byte buffers'; 'Arena network connection' with subtext 'WebSocket' and 'Negotiated JSON / MessagePack'. Bottom narrow note 'OSC-IR preserves message and argument order'. No native struct interface, no zero-serialization claim, no invented addresses, no nested payload example. Reference image is STYLE ONLY; generate new composition and content.

### `unity-client-presentation-layer.png`

Title: 'Unity Client'. Subtitle: 'Input and presentation around an authoritative Rust application'. Left blue panel 'Unity owns' with four rows 'Device input and camera', 'Visuals / audio / UI', 'Addressables and assets', 'Presentation object lifetime'. Right violet panel 'Rust application owns' with four rows 'Game rules and state', 'Ordered input application', 'Content activation', 'Authoritative output'. Two separate clear arrows between panels: left to right 'Typed input'; right to left 'State and events'. Below two teal cards 'Standalone mode' / 'Connect to application server' and 'Embedded mode' / 'Schedule native Runtime locally'. Bottom amber bar 'Maintained reference: kitu-unity-demo-game'. No claim every object needs a spawn event; no all-state-machines prohibited claim; no rollback/prediction claims. Reference image is STYLE ONLY; generate new composition and content.

### `kitu-shell.png`

Title: 'Kitu Shell'. Subtitle: 'Shared command contracts for CLI and browser tools'. Top two small amber source cards 'CLI' and 'Browser Admin' feeding downward via arrows into one amber center card 'Shared Shell contracts' containing 'Catalog / parsing / validation' and 'Request identity / results'. Center card arrow downward labeled 'Host request' to violet wide card 'Application host' with 'Admission → Authoritative tick → Applied receipt'. Bottom three teal feature cards 'Inspect state', 'Validate and stage content', 'Control recording and replay'. Separate compact note 'Available commands come from the connected host'. Include readable small standalone terminal-style box with exactly two lines 'kitu-cli --endpoint http://127.0.0.1:8787 help' and 'kitu-cli --endpoint http://127.0.0.1:8787 shell'. Position terminal box where text fits with large readable monospace, no fake screenshot, no invented command syntax. Small footnote 'Standalone host or optional embedded development bridge'. Shared contracts serve both clients, no arrow from browser to CLI. Reference image is STYLE ONLY; generate new composition and content.

### `data-systems-tmd-sqlite.png`

Title: 'Data Systems'. Subtitle: 'Prepare typed content outside the simulation tick'. Upper two teal cards 'Tanu documents' with 'ZIP container + named tables' and 'Public tmd-core evaluation'; 'SQLite snapshots' with 'Bounded read-only transaction' and 'Typed detached values'. Both arrows downward converge into one violet wide panel 'Application validation' with labels 'Schema and domain rules', 'Layering and provenance', 'Activation policy'. Then arrow down to amber bar 'Arena workflow: validate → stage → next run'. Below two side-by-side blue cards 'Active run' / 'Retains original content' and 'Recording' / 'Retains content identity'. Small footer note above standard footer 'kitu-calculation: shared OpenFormula registry + KITU.* extensions'. No TMD plain text or YAML, no direct unvalidated live reload, no direct DB access from Unity, no invented table data. Reference image is STYLE ONLY; generate new composition and content.

### `timeline-and-automation-tsq1.png`

Title: 'Timeline and Replay'. Subtitle: 'Binary TSQ1 with explicit tick and event order'. Two large equal panels with distinct teal/violet border. Left teal heading 'Presentation clips' three rows 'Exact integer tick offsets', 'Ordered OSC bundles', 'Application-owned clock and cursor'. Below rows a simple horizontal tick line with four equal ticks '0', '1', '2', '3', with two event dots at 1 and 3. Right violet heading 'Session recordings' three rows 'Committed input tick and order', 'Application-owned manifest', 'Matching code and content'. Below rows three small connected boxes 'Record' → 'Re-execute' → 'Compare'. Below both panels a wide blue bar 'Engine presentation' with 'Consumes events and projects replay state'. Bottom amber bar 'Interpolation and easing belong to application / presentation systems'. Small note 'Arena: boss warnings, floor transitions, and replay controls'. No seconds timing, markers, JSON schema, built-in tween/easing, or visual editor claim. Reference image is STYLE ONLY; generate new composition and content.

## Reviewed corrections

### Unity host modes

Edit only the two teal panels in the lower third of this Unity Client infographic. Preserve every other pixel, title, colors, panel layout, typography, arrows, blue/violet upper panels, amber footer and all other content. Correct the LEFT teal card: heading must be exactly 'Standalone mode'; its body must be exactly 'Connect to application server', with a server icon. Correct the RIGHT teal card: heading must be exactly 'Embedded mode'; its body must be exactly 'Schedule native Runtime locally', with a laptop icon. These are two distinct modes; never repeat body text or use body text as a heading. Keep all text fully inside the panels. Output the same 1536x1024 size.

### Generic C ABI encoding

Edit only the bottom-left teal panel in this Communication Layer infographic. Preserve all other content, colors, typography, layout, and arrows exactly. Change heading 'Embedded connection' to 'Generic application C ABI'. Keep 'Versioned C ABI' inside the card unchanged. Replace body text 'Typed JSON / MessagePack byte buffers' with exactly 'Typed JSON byte buffers'. The bottom-right Arena network connection remains 'WebSocket' and 'Negotiated JSON / MessagePack', unchanged. Same 1536x1024 landscape image.
