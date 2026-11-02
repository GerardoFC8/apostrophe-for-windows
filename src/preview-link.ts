// Piezas de los enlaces del preview que comparten el panel y la ventana desacoplada. Viven
// aparte para que la ventana desacoplada no arrastre el editor al importarlas.

// Enlaces del preview. El iframe usa srcdoc, asi que dejarlo navegar rompe el panel: un
// `notas.md` relativo se resuelve contra la URL de la app y el iframe termina mostrando la
// interfaz de Elision. Este script viaja DENTRO del HTML del preview, intercepta el clic y
// pasa el href a la ventana que lo contiene por postMessage, el unico canal posible: sin
// allow-same-origin, ni la ventana puede leer el iframe ni el iframe a la ventana.
//
// Las anclas (`#fn1` de las notas al pie) se resuelven ahi mismo, porque el elemento al que
// apuntan vive en el iframe. El clic central tambien se anula: intentaria abrir otra ventana.
export const LINK_SCRIPT = `<script>
(function () {
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
    parent.postMessage({ elisionLink: href }, "*");
  });
  document.addEventListener("auxclick", function (e) {
    if (linkOf(e)) e.preventDefault();
  });
})();
</script>`;

// href que el iframe pide seguir, si el mensaje viene de ahi. El origen se comprueba con
// `source` porque el iframe sandboxeado tiene origen opaco ("null"): no hay otro dato fiable.
export function linkFromMessage(e: MessageEvent, frame: HTMLIFrameElement): string | null {
  if (e.source !== frame.contentWindow) return null;
  const href = (e.data as { elisionLink?: unknown } | null)?.elisionLink;
  return typeof href === "string" ? href : null;
}
