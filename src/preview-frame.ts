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
// - Scroll sincronizado. La ventana manda qué renglón del Markdown poner arriba
//   (scroll-sync.ts); el iframe lo traduce a su propio desplazamiento con los
//   `data-sourcepos` de los bloques. Al cargar avisa que está listo.
// - Actualizaciones. Después de cargar, la ventana manda solo el `<body>` nuevo y aquí se
//   cambian los bloques distintos (ver `frameUpdater`).
import type { SyncPosition } from "./scroll-sync";

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

  // Scroll sincronizado por renglón (ADR-0003). Cada bloque trae "data-sourcepos", su
  // renglón en el Markdown. Se arma una tabla de puntos (renglón, altura) y entre dos puntos
  // se interpola. Se mide una vez y se reutiliza en cada cuadro: leer getBoundingClientRect
  // al desplazar fuerza un layout por cuadro, y en documentos largos o con imágenes eso se
  // nota como un retraso. Se vuelve a medir solo cuando cambia la altura (imágenes o
  // fórmulas que terminan de cargar, ventana más angosta) o el contenido.
  //
  // Bloques "hoja": aportan su principio y su final, así un párrafo o un bloque de código
  // largo se recorre a la par. Los contenedores (listas, citas, tablas) solo su principio: su
  // final viene después que el de sus hijos, que ya lo cubren.
  var LEAF = { P: 1, H1: 1, H2: 1, H3: 1, H4: 1, H5: 1, H6: 1, PRE: 1, TR: 1, HR: 1, DT: 1, DD: 1, FIGURE: 1 };
  var CONTAINER = { UL: 1, OL: 1, LI: 1, BLOCKQUOTE: 1, TABLE: 1, DL: 1, DIV: 1 };
  // Doble barra: esto es un template literal de TypeScript, y "\d" llegaría como "d".
  var POS = /^(\\d+):\\d+-(\\d+):\\d+$/;
  var lastSync = null, points = null, max = 0;
  function measure() {
    points = [];
    var lastLine = -1, lastY = -1;
    // Solo se aceptan puntos que avanzan en los dos ejes: las notas al pie se escriben
    // antes pero se muestran al final, y desordenarían la tabla.
    function add(line, y) {
      if (line >= lastLine && y >= lastY) { points.push([line, y]); lastLine = line; lastY = y; }
    }
    var els = document.querySelectorAll("[data-sourcepos]");
    for (var i = 0; i < els.length; i++) {
      var el = els[i];
      var leaf = LEAF[el.tagName] || el.classList.contains("mermaid-diagram");
      if (!leaf && !CONTAINER[el.tagName]) continue;
      var m = POS.exec(el.getAttribute("data-sourcepos"));
      if (!m) continue;
      var r = el.getBoundingClientRect();
      var top = r.top + window.scrollY - 12;
      add(Number(m[1]) - 1, top);
      if (leaf) add(Number(m[2]), r.bottom + window.scrollY - 12);
    }
    max = document.documentElement.scrollHeight - window.innerHeight;
  }
  function table() {
    if (!points) measure();
    return points;
  }
  // Último punto con "pts[i][k] <= value" (k = 0 renglón, 1 altura), o -1.
  function floorIndex(pts, k, value) {
    var lo = 0, hi = pts.length - 1, found = -1;
    while (lo <= hi) {
      var mid = (lo + hi) >> 1;
      if (pts[mid][k] <= value) { found = mid; lo = mid + 1; } else hi = mid - 1;
    }
    return found;
  }
  // Interpola en la tabla: de renglón a altura (from 0, to 1) o al revés (from 1, to 0).
  function lookup(value, from, to) {
    var pts = table();
    if (!pts.length) return 0;
    var i = floorIndex(pts, from, value);
    var a = i < 0 ? [0, 0] : pts[i];
    var b = i + 1 < pts.length ? pts[i + 1] : null;
    if (!b || b[from] <= a[from]) return a[to];
    return a[to] + (b[to] - a[to]) * ((value - a[from]) / (b[from] - a[from]));
  }
  function invalidate() {
    points = null;
    if (lastSync) applySync(lastSync);
  }
  // Destino del último desplazamiento ordenado desde afuera: su evento de scroll no se
  // informa de vuelta (la ventana ya sabe dónde quedó) y se ahorra un mensaje por cuadro.
  var applied = -1;
  function applySync(s) {
    lastSync = s;
    table();
    var target = s.end ? max : lookup(s.line, 0, 1);
    target = Math.round(Math.min(max, Math.max(0, target)));
    if (Math.abs(window.scrollY - target) < 1) return;
    applied = target;
    window.scrollTo(0, target);
  }

  // Actualización sin recrear el documento: la ventana manda solo el cuerpo nuevo y aquí se
  // reemplazan los bloques que cambiaron. Se compara desde el principio y desde el final (una
  // edición suele tocar un solo lugar); a los bloques iguales solo se les copia el
  // "data-sourcepos", que cambia en todos los que siguen a un renglón agregado. Así no hay
  // parpadeo, el scroll no se pierde y las imágenes que no cambiaron no se decodifican otra vez.
  // Lo que no cuenta al comparar: el renglón de origen y el "decoding" que este mismo script
  // les pone a las imágenes (si contara, ningún bloque con una imagen volvería a coincidir).
  var NOISE = / (data-sourcepos|decoding)="[^"]*"/g;
  function keyOf(node) {
    // Los espacios entre bloques no se ven, y al cargar el documento completo el parser pega
    // al final del body el salto que sigue al cierre del body: si contaran, la comparación desde el
    // final fallaría siempre en el último nodo.
    if (node.nodeType === 3 && node.nodeValue.trim() === "") return "#ws";
    if (node.nodeType !== 1) return "#" + node.nodeType + ":" + node.nodeValue;
    if (node.__elisionKey === undefined) node.__elisionKey = node.outerHTML.replace(NOISE, "");
    return node.__elisionKey;
  }
  function copySourcepos(from, to) {
    if (from.nodeType !== 1) return;
    var a = [from].concat(Array.prototype.slice.call(from.querySelectorAll("[data-sourcepos]")));
    var b = [to].concat(Array.prototype.slice.call(to.querySelectorAll("[data-sourcepos]")));
    for (var i = 0; i < a.length && i < b.length; i++) {
      var v = a[i].getAttribute("data-sourcepos");
      if (v !== null && b[i].getAttribute("data-sourcepos") !== v) b[i].setAttribute("data-sourcepos", v);
    }
  }
  function updateBody(html) {
    closeImage();
    var tpl = document.createElement("template");
    tpl.innerHTML = html;
    var next = Array.prototype.slice.call(tpl.content.childNodes);
    var body = document.body;
    var prev = Array.prototype.slice.call(body.childNodes);
    var start = 0;
    while (start < prev.length && start < next.length && keyOf(prev[start]) === keyOf(next[start])) {
      copySourcepos(next[start], prev[start]);
      start++;
    }
    var pe = prev.length - 1, ne = next.length - 1;
    while (pe >= start && ne >= start && keyOf(prev[pe]) === keyOf(next[ne])) {
      copySourcepos(next[ne], prev[pe]);
      pe--; ne--;
    }
    var ref = prev[pe + 1] || null;
    for (var i = start; i <= pe; i++) body.removeChild(prev[i]);
    for (var j = start; j <= ne; j++) {
      var node = next[j];
      if (node.nodeType === 1) {
        var imgs = node.tagName === "IMG" ? [node] : node.querySelectorAll("img");
        for (var k = 0; k < imgs.length; k++) imgs[k].decoding = "async";
      }
      body.insertBefore(node, ref);
    }
    invalidate();
  }

  window.addEventListener("message", function (e) {
    if (e.source !== parent || !e.data) return;
    if (e.data.elisionSync) applySync(e.data.elisionSync);
    else if (typeof e.data.elisionBody === "string") updateBody(e.data.elisionBody);
  });
  // La posición propia, en renglones: la ventana la guarda para restaurarla cuando el iframe
  // se recargue, también si se desplazó a mano, y con ella marca la sección en el esquema.
  var reporting = false;
  window.addEventListener("scroll", function () {
    if (reporting) return;
    reporting = true;
    requestAnimationFrame(function () {
      reporting = false;
      var y = window.scrollY;
      // El eco se descarta una sola vez: el siguiente desplazamiento ya es del usuario.
      var echo = applied >= 0 && Math.abs(y - applied) < 2;
      applied = -1;
      if (echo) return;
      table();
      // Lo informado pasa a ser lo que se conserva si cambia la altura (una imagen que
      // termina de cargar no debe devolver la vista a donde la dejó el editor).
      if (y > 0 && y >= max - 2) {
        lastSync = { line: 0, end: true };
        post({ elisionScrolled: lastSync });
        return;
      }
      lastSync = { line: lookup(y, 1, 0) };
      // El renglón que se lee, para el esquema: el que está en el primer quinto de la vista
      // (misma regla que el editor, scroll-sync.ts).
      var reading = lookup(y + Math.max(48, window.innerHeight * 0.2), 1, 0);
      post({ elisionScrolled: { line: lastSync.line, reading: reading } });
    });
  }, { passive: true });
  window.addEventListener("load", invalidate);
  window.addEventListener("resize", invalidate);
  window.__elisionResync = invalidate;
  document.addEventListener("DOMContentLoaded", function () {
    // Las imágenes van incrustadas (data URI): decodificarlas fuera del hilo principal evita
    // que una foto grande frene el desplazamiento.
    var imgs = document.images;
    for (var i = 0; i < imgs.length; i++) imgs[i].decoding = "async";
    if (window.ResizeObserver) {
      var lastHeight = -1;
      new ResizeObserver(function () {
        var h = document.documentElement.scrollHeight;
        if (h !== lastHeight) { lastHeight = h; invalidate(); }
      }).observe(document.body);
    }
  });
  document.addEventListener("DOMContentLoaded", function () { post({ elisionReady: true }); });
})();
</script>`;

/// Lo que el iframe pide a la ventana que lo contiene.
export type FrameMessage =
  | { kind: "ready" }
  | { kind: "scrolled"; pos: SyncPosition }
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
  if (pos && typeof pos.line === "number" && Number.isFinite(pos.line)) {
    const reading = typeof pos.reading === "number" && Number.isFinite(pos.reading) ? pos.reading : undefined;
    return { kind: "scrolled", pos: { line: pos.line, end: pos.end === true, reading } };
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

/// Lo que muestra un iframe de vista previa (el panel o la ventana desacoplada), sin
/// recrearlo en cada render.
export interface FrameUpdater {
  /** Muestra el documento HTML completo `html`. */
  show(html: string): void;
  /** El iframe avisó que cargó (`ready`): desde ahí acepta cuerpos nuevos. */
  markReady(): void;
}

/// Recrea el iframe (`srcdoc`) solo la primera vez y cuando cambia el `<head>` (tema, zoom):
/// el resto de las veces le manda el `<body>` nuevo y el script de adentro cambia solo los
/// bloques distintos. Recrearlo en cada render hacía parpadear la vista previa, la llevaba
/// arriba hasta que llegaba la posición y volvía a decodificar todas las imágenes.
export function frameUpdater(frame: HTMLIFrameElement): FrameUpdater {
  let head = "";
  let ready = false;
  return {
    show(html) {
      const headEnd = html.indexOf("</head>");
      // El <body> se busca DESPUÉS del head: el head lleva este mismo script y sus estilos, y
      // cualquier "<body>" escrito ahí (en un comentario, por ejemplo) se tomaría por el real.
      const bodyStart = html.indexOf("<body>", Math.max(0, headEnd));
      const bodyEnd = html.lastIndexOf("</body>");
      const nextHead = headEnd >= 0 ? html.slice(0, headEnd) : "";
      if (ready && nextHead === head && bodyStart >= 0 && bodyEnd > bodyStart) {
        frame.contentWindow?.postMessage({ elisionBody: html.slice(bodyStart + "<body>".length, bodyEnd) }, "*");
        return;
      }
      head = nextHead;
      ready = false;
      frame.srcdoc = html;
    },
    markReady() {
      ready = true;
    },
  };
}
