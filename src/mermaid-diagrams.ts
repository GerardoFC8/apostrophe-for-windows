// Diagramas Mermaid en la vista previa. Un bloque ```mermaid sale de pandoc como
// `<pre class="mermaid"><code>…</code></pre>`; aquí se reemplaza por el SVG del diagrama
// antes de que el HTML llegue a la vista previa.
//
// Se dibuja en la ventana principal y no dentro del iframe: Mermaid pesa varios megas, y el
// iframe se vuelve a crear en cada render (cada vez que se escribe). Aquí se carga una sola
// vez, y solo si el documento tiene algún diagrama (import dinámico: otro archivo del
// bundle). Los diagramas que no cambiaron salen de una caché. Como el SVG viaja dentro del
// HTML, la ventana desacoplada los recibe sin hacer nada.

type MermaidApi = typeof import("mermaid").default;

const BLOCK = /<pre class="mermaid"><code>([\s\S]*?)<\/code><\/pre>/g;
/// Tope de la caché: una entrada por diagrama distinto (código + tema).
const CACHE_LIMIT = 200;

let api: Promise<MermaidApi> | null = null;
let configuredTheme = "";
const cache = new Map<string, string>();
let counter = 0;

function loadMermaid(): Promise<MermaidApi> {
  api ??= import("mermaid").then((m) => m.default);
  return api;
}

// El texto del bloque viene escapado como HTML (`--&gt;`): se decodifica para Mermaid.
function decode(html: string): string {
  const doc = new DOMParser().parseFromString(`<pre>${html}</pre>`, "text/html");
  return doc.body.textContent ?? "";
}

function escapeHtml(s: string): string {
  return s.replace(/[&<>]/g, (c) => (c === "&" ? "&amp;" : c === "<" ? "&lt;" : "&gt;"));
}

async function renderOne(mermaid: MermaidApi, code: string, theme: string): Promise<string> {
  const key = `${theme}\n${code}`;
  const cached = cache.get(key);
  if (cached) return cached;
  const id = `elision-mermaid-${++counter}`;
  let out: string;
  try {
    const { svg } = await mermaid.render(id, code);
    out = `<div class="mermaid-diagram">${svg}</div>`;
  } catch (err) {
    // Un diagrama con errores se deja como código, con el motivo debajo: así se puede corregir.
    document.getElementById(`d${id}`)?.remove();
    const message = err instanceof Error ? err.message : String(err);
    out =
      `<pre class="mermaid-error"><code>${escapeHtml(code)}</code></pre>` +
      `<p class="mermaid-error-message">${escapeHtml(message)}</p>`;
  }
  if (cache.size >= CACHE_LIMIT) cache.delete(cache.keys().next().value as string);
  cache.set(key, out);
  return out;
}

/// `html` con cada bloque Mermaid reemplazado por su diagrama. `dark` elige el tema de los
/// diagramas. Sin bloques Mermaid devuelve el mismo texto sin cargar nada.
export async function renderMermaid(html: string, dark: boolean): Promise<string> {
  const blocks = [...html.matchAll(BLOCK)];
  if (blocks.length === 0) return html;
  const mermaid = await loadMermaid();
  const theme = dark ? "dark" : "default";
  if (theme !== configuredTheme) {
    mermaid.initialize({ startOnLoad: false, theme, securityLevel: "strict" });
    configuredTheme = theme;
  }
  // En orden: Mermaid no admite dibujar dos diagramas a la vez.
  const rendered: string[] = [];
  for (const block of blocks) rendered.push(await renderOne(mermaid, decode(block[1]), theme));
  let i = 0;
  return html.replace(BLOCK, () => rendered[i++]);
}
