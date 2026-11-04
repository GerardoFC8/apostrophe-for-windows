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
// - Scroll sincronizado. La ventana manda qué título poner arriba y cuánto avanzar hacia el
//   siguiente (scroll-sync.ts); el iframe lo traduce a su propio desplazamiento. Al cargar
//   avisa que está listo, porque cada render lo vuelve a crear desde arriba.
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

  var lastSync = null;
  function headings() {
    return Array.prototype.filter.call(
      document.querySelectorAll("h1,h2,h3,h4,h5,h6"),
      function (h) { return !h.closest("#title-block-header"); });
  }
  function topOf(el) {
    return el.getBoundingClientRect().top + window.scrollY - 12;
  }
  function applySync(s) {
    lastSync = s;
    var max = document.documentElement.scrollHeight - window.innerHeight;
    if (s.end) { window.scrollTo(0, max); return; }
    var hs = headings();
    var start = s.index >= 0 && hs[s.index] ? topOf(hs[s.index]) : 0;
    var end = hs[s.index + 1] ? topOf(hs[s.index + 1]) : max;
    var target = start + Math.max(0, end - start) * s.fraction;
    window.scrollTo(0, Math.min(max, Math.max(0, target)));
  }
  window.addEventListener("message", function (e) {
    if (e.source === parent && e.data && e.data.elisionSync) applySync(e.data.elisionSync);
  });
  // La posición propia, en los mismos términos (título + fracción): la ventana la guarda para
  // restaurarla cuando el iframe se recargue, también si se desplazó a mano.
  var reporting = false;
  window.addEventListener("scroll", function () {
    if (reporting) return;
    reporting = true;
    requestAnimationFrame(function () {
      reporting = false;
      var max = document.documentElement.scrollHeight - window.innerHeight;
      if (window.scrollY > 0 && window.scrollY >= max - 2) {
        post({ elisionScrolled: { index: -1, fraction: 1, end: true } });
        return;
      }
      var hs = headings(), index = -1;
      for (var i = 0; i < hs.length && topOf(hs[i]) <= window.scrollY + 1; i++) index = i;
      var start = index >= 0 ? topOf(hs[index]) : 0;
      var end = hs[index + 1] ? topOf(hs[index + 1]) : max;
      var fraction = end > start ? Math.min(1, Math.max(0, (window.scrollY - start) / (end - start))) : 0;
      post({ elisionScrolled: { index: index, fraction: fraction } });
    });
  }, { passive: true });
  // Las imágenes y las fórmulas cambian la altura al terminar de cargar: se reaplica.
  window.addEventListener("load", function () { if (lastSync) applySync(lastSync); });
  window.__elisionResync = function () { if (lastSync) applySync(lastSync); };
  document.addEventListener("DOMContentLoaded", function () { post({ elisionReady: true }); });
})();
</script>`;

/// Lo que el iframe pide a la ventana que lo contiene.
export type FrameMessage =
  | { kind: "ready" }
  | { kind: "scrolled"; pos: { index: number; fraction: number; end?: boolean } }
  | { kind: "link"; href: string }
  | { kind: "zoom"; step: 1 | -1 | 0 }
  | { kind: "wheel"; deltaY: number };

// Mensaje del iframe, si viene de ahí. El origen se comprueba con `source` porque el iframe
// sandboxeado tiene origen opaco ("null"): no hay otro dato fiable.
export function frameMessage(e: MessageEvent, frame: HTMLIFrameElement): FrameMessage | null {
  if (e.source !== frame.contentWindow) return null;
  const data = e.data as Record<string, unknown> | null;
  if (data?.elisionReady === true) return { kind: "ready" };
  const pos = data?.elisionScrolled as Record<string, unknown> | undefined;
  if (pos && typeof pos.index === "number" && typeof pos.fraction === "number") {
    return { kind: "scrolled", pos: { index: pos.index, fraction: pos.fraction, end: pos.end === true } };
  }
  if (typeof data?.elisionLink === "string") return { kind: "link", href: data.elisionLink };
  if (data?.elisionZoom === 1 || data?.elisionZoom === -1 || data?.elisionZoom === 0) {
    return { kind: "zoom", step: data.elisionZoom };
  }
  if (typeof data?.elisionWheel === "number" && Number.isFinite(data.elisionWheel)) {
    return { kind: "wheel", deltaY: data.elisionWheel };
  }
  return null;
}
