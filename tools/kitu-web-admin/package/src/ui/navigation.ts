export function resolveAdminHref(href: string, basePath = '') {
  if (!href.startsWith('/') || href.startsWith('//')) return href
  const baseSegments = basePath.split('/').filter(Boolean)
  const normalizedBase = baseSegments.length === 0 ? '' : `/${baseSegments.join('/')}`
  return href === '/' ? `${normalizedBase}/` : `${normalizedBase}${href}`
}

export function isAdminHrefActive(currentPath: string, href: string, basePath = '') {
  const target = resolveAdminHref(href, basePath)
  return currentPath === target || (href === '/' && target !== '/' && currentPath === target.slice(0, -1))
}
