import { type Component } from 'svelte';
import type { AdminClient } from '../client/create-admin-client.js';
export type AdminNavItem = {
    href: string;
    label: string;
    icon: Component<{
        size?: number;
    }>;
};
export type AdminNavSection = {
    id: string;
    label: string;
    items: AdminNavItem[];
};
type $$ComponentProps = {
    client: AdminClient;
    currentPath: string;
    sections: AdminNavSection[];
    brand: string;
    subtitle: string;
    basePath?: string;
    showRuntimeStatus?: boolean;
    header?: import('svelte').Snippet;
    children?: import('svelte').Snippet;
};
declare const AdminShell: Component<$$ComponentProps, {}, "">;
type AdminShell = ReturnType<typeof AdminShell>;
export default AdminShell;
