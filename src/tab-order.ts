// Reglas PURAS de las pestañas: orden, vecina al cerrar, siguiente/anterior y cuándo
// reutilizar una pestaña vacía. Sin DOM, sin IPC y sin CodeMirror: solo datos de entrada y
// de salida, para que se puedan razonar (y probar) de forma aislada.

/// Pestaña que queda activa al cerrar `closing`. Espejo de `Workspace::close` del núcleo:
/// la que ocupa su lugar (la de la derecha) y, si era la última, la de la izquierda.
/// Si la cerrada no era la activa, la activa no cambia. `null` si no queda ninguna.
export function activeAfterClose<T>(ids: readonly T[], closing: T, active: T | null): T | null {
  const rest = ids.filter((id) => id !== closing);
  if (rest.length === 0) return null;
  if (active !== closing && active !== null && rest.includes(active)) return active;
  const index = ids.indexOf(closing);
  if (index < 0) return rest[0];
  return rest[Math.min(index, rest.length - 1)];
}

/// Pestaña siguiente (`step = 1`) o anterior (`step = -1`) a `active`, con vuelta al
/// principio/final. Con una sola pestaña devuelve esa misma; sin pestañas, `null`.
export function cycleTab<T>(ids: readonly T[], active: T | null, step: 1 | -1): T | null {
  if (ids.length === 0) return null;
  const index = active === null ? -1 : ids.indexOf(active);
  if (index < 0) return ids[0];
  return ids[(index + step + ids.length) % ids.length];
}

/// Lo que hace falta saber de una pestaña para decidir si se puede reutilizar.
export interface ReuseCandidate {
  path: string | null;
  dirty: boolean;
  textLength: number;
}

/// ¿Abrir un archivo puede ocupar esta pestaña en vez de crear otra? Solo si es una
/// pestaña "en blanco": sin archivo, sin texto y sin cambios. Es el caso del documento
/// vacío con el que arranca la app: abrir algo no debe dejar una pestaña vacía colgando.
export function canReuseTab(tab: ReuseCandidate | null | undefined): boolean {
  return !!tab && tab.path === null && !tab.dirty && tab.textLength === 0;
}
