// Barra de estado de la titlebar: feedback de accion (guardado, exportado, error...).
// Modulo hoja: lo usan casi todos los demas, y el no importa a ninguno de la app.

const status = document.querySelector<HTMLElement>("#status")!;

// Estado TRANSITORIO en la titlebar: muestra feedback de accion (guardado, exportado,
// error...) y se auto-limpia. En reposo la barra queda limpia (no un "listo" permanente),
// coherente con el ethos distraction-free de Apostrophe.
let statusTimer: number | undefined;
export function flashStatus(msg: string, ms = 1800): void {
  status.textContent = msg;
  if (statusTimer !== undefined) clearTimeout(statusTimer);
  statusTimer = window.setTimeout(() => {
    status.textContent = "";
  }, ms);
}

// Texto FIJO en la barra (sin auto-limpiar), para acciones en curso como exportar. Quien lo
// pone es responsable de limpiarlo (pasando "") o de reemplazarlo con flashStatus.
export function showStatus(msg: string): void {
  status.textContent = msg;
}
