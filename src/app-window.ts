import { getCurrentWindow } from "@tauri-apps/api/window";

// Chrome nativo (M3): titlebar custom. `decorations: false` (tauri.conf.json) saca
// la barra Win32; la ventana se maneja via la window API de Tauri. Una sola instancia
// compartida por todos los modulos (cierre, pantalla completa, botones de la titlebar).
export const appWindow = getCurrentWindow();
