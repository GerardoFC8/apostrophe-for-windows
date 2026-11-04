// Script que viaja DENTRO del HTML de la vista previa, y lo que necesitan el panel y la
// ventana desacoplada para hablar con él. Vive aparte para que la ventana desacoplada no
// arrastre el editor al importarlo.
//
// El preview es un iframe con srcdoc y sandbox sin allow-same-origin: ni la ventana puede
// leer el iframe ni el iframe a la ventana. Lo que pasa adentro (un clic en un enlace, un
// Ctrl + rueda) solo llega afuera por postMessage, y eso es lo que hace este script:
//
// - Enlaces. Dejar navegar al iframe rompe el panel: un `notas.md` relativo se resuelve
//   contra la URL de la app y el iframe termina mostrando la interfaz de Elision. Se
//   intercepta el clic y se pasa el href. Las anclas (`#fn1` de las notas al pie) se
//   resuelven ahí mismo, porque el elemento al que apuntan vive en el iframe. El clic
//   central se anula: intentaría abrir otra ventana.
// - Zoom. Con el mouse o el foco sobre la vista previa, Ctrl + rueda y Ctrl + / - / 0 los
//   recibe el iframe, no la ventana: se reenvían para que el zoom funcione igual que en el
//   editor.
// - Imágenes. Doble clic sobre una imagen la amplía al tamaño de la vista; clic o Esc la
//   cierra. Todo dentro del iframe: no hace falta salir de él.
export const FRAME_SCRIPT = `<script>
(function () {
  function post(data) {
    parent.postMessage(data, "*");
  }
  function linkOf(e) {
    return e.target instanceof Element ? e.target.closest("a[href]") : null;
  }
  document.addEventListener("click", function (e) {
    var a = linkOf(e);
    if (!a) return;
    e.preventDefault();
    var href = a.getAttribute("href");
    if (href.charAt(0) === "#") {
      var id = href.slice(1);
      try { id = decodeURIComponent(id); } catch (_) {}
      var target = document.getElementById(id);
      if (target) target.scrollIntoView();
      return;
    }
    post({ elisionLink: href });
  });
  document.addEventListener("auxclick", function (e) {
    if (linkOf(e)) e.preventDefault();
  });

  window.addEventListener("wheel", function (e) {
    if (!e.ctrlKey) return;
    e.preventDefault();
    post({ elisionWheel: e.deltaY });
  }, { passive: false });
  window.addEventListener("keydown", function (e) {
    if (!(e.ctrlKey || e.metaKey) || e.altKey) return;
    var step = e.key === "+" || e.key === "=" ? 1 : e.key === "-" ? -1 : e.key === "0" ? 0 : null;
    if (step === null) return;
    e.preventDefault();
    post({ elisionZoom: step });
  }, true);

  var overlay = null;
  function closeImage() {
    if (overlay) { overlay.remove(); overlay = null; }
  }
  document.addEventListener("dblclick", function (e) {
    var img = e.target instanceof HTMLImageElement ? e.target : null;
    if (!img || overlay) return;
    e.preventDefault();
    overlay = document.createElement("div");
    overlay.setAttribute("style",
      "position:fixed;inset:0;z-index:2147483647;display:flex;align-items:center;" +
      "justify-content:center;background:rgba(0,0,0,0.85);cursor:zoom-out;");
    var big = document.createElement("img");
    big.src = img.currentSrc || img.src;
    big.alt = img.alt;
    big.setAttribute("style",
      "width:95vw;height:95vh;object-fit:contain;box-shadow:0 8px 32px rgba(0,0,0,0.5);");
    overlay.appendChild(big);
    overlay.addEventListener("click", closeImage);
    document.body.appendChild(overlay);
  });
  document.addEventListener("keydown", function (e) {
    if (e.key === "Escape") closeImage();
  });
})();
</script>`;

/// Lo que el iframe pide a la ventana que lo contiene.
export type FrameMessage =
  | { kind: "link"; href: string }
  | { kind: "zoom"; step: 1 | -1 | 0 }
  | { kind: "wheel"; deltaY: number };

// Mensaje del iframe, si viene de ahí. El origen se comprueba con `source` porque el iframe
// sandboxeado tiene origen opaco ("null"): no hay otro dato fiable.
export function frameMessage(e: MessageEvent, frame: HTMLIFrameElement): FrameMessage | null {
  if (e.source !== frame.contentWindow) return null;
  const data = e.data as Record<string, unknown> | null;
  if (typeof data?.elisionLink === "string") return { kind: "link", href: data.elisionLink };
  if (data?.elisionZoom === 1 || data?.elisionZoom === -1 || data?.elisionZoom === 0) {
    return { kind: "zoom", step: data.elisionZoom };
  }
  if (typeof data?.elisionWheel === "number" && Number.isFinite(data.elisionWheel)) {
    return { kind: "wheel", deltaY: data.elisionWheel };
  }
  return null;
}
