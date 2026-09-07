export function resolveAdminHref(href, basePath = '') {
    if (!href.startsWith('/') || href.startsWith('//'))
        return href;
    const normalizedBase = basePath === '/' ? '' : `/${basePath.split('/').filter(Boolean).join('/')}`;
    return href === '/' ? `${normalizedBase}/` : `${normalizedBase}${href}`;
}
export function isAdminHrefActive(currentPath, href, basePath = '') {
    const target = resolveAdminHref(href, basePath);
    return currentPath === target || (href === '/' && target !== '/' && currentPath === target.slice(0, -1));
}
