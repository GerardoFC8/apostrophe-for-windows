// Ventana de vista previa desacoplada (M18). Paridad con `preview_window.py` del upstream:
// la vista renderizada en su propia ventana, para escribir en un monitor y leer en el otro.
//
// Es DELIBERADAMENTE tonta: no habla con el dominio, no invoca comandos, no conoce temas ni
// idiomas. La ventana principal ya calcula el HTML del preview (con su hoja de estilos
// inyectada) y lo emite; aquí solo se vuelca en el iframe. Toda la lógica sigue viviendo en
// un solo lado.

import { listen, emit } from "@tauri-apps/api/event";

const frame = document.querySelector<HTMLIFrameElement>("#frame")!;

// La ventana principal emite esto en cada render (y una vez al abrirse esta ventana).
void listen<string>("preview-html", (event) => {
  frame.srcdoc = event.payload;
});

// Al abrir, la ventana principal puede llevar rato sin renderizar: se le pide el HTML
// actual para no quedarse en blanco hasta la próxima tecla.
void emit("preview-window-ready");
