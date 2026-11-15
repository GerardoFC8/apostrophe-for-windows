// Resaltado de código en la vista previa (ADR-0003). comrak deja cada bloque como
// `<pre …><code class="language-rust">…</code></pre>`; aquí se colorea con highlight.js antes
// de que el HTML llegue al iframe.
//
// Mismo criterio que Mermaid (mermaid-diagrams.ts): corre en la ventana principal, la
// biblioteca se carga una sola vez y solo si el documento tiene un bloque con lenguaje
// (import dinámico: otro archivo del bundle). Como el resultado viaja dentro del HTML, la
// ventana desacoplada lo recibe sin hacer nada. Se resalta aquí y no en Rust porque syntect
// sumaría unos 2 MB al ejecutable solo para colorear (ver el ADR).

type HljsApi = typeof import("highlight.js/lib/common").default;

// Bloque con lenguaje. `[^>]*` deja pasar `data-sourcepos` y los atributos `{…}` del bloque.
const BLOCK = /<pre([^>]*)><code class="language-([^"\s]+)"([^>]*)>([\s\S]*?)<\/code><\/pre>/g;

/// Tope de la caché: una entrada por bloque distinto (lenguaje + código).
const CACHE_LIMIT = 200;

let api: Promise<HljsApi> | null = null;
// Se renderiza en cada pausa al escribir: los bloques que no cambiaron salen de aquí.
const cache = new Map<string, string>();

function loadHljs(): Promise<HljsApi> {
  api ??= import("highlight.js/lib/common").then((m) => m.default);
  return api;
}

// El código viene escapado como HTML; highlight.js necesita el texto y devuelve HTML escapado.
// Va en un <div> y no en un <pre>: el parser HTML descarta el salto de línea que sigue a <pre>,
// y un bloque que empieza con un renglón en blanco lo perdería.
function decode(html: string): string {
  const doc = new DOMParser().parseFromString(`<div>${html}</div>`, "text/html");
  return doc.body.textContent ?? "";
}

function highlightOne(hljs: HljsApi, lang: string, body: string): string {
  const key = `${lang}\n${body}`;
  const cached = cache.get(key);
  if (cached !== undefined) return cached;
  const { value } = hljs.highlight(decode(body), { language: lang, ignoreIllegals: true });
  if (cache.size >= CACHE_LIMIT) cache.delete(cache.keys().next().value as string);
  cache.set(key, value);
  return value;
}

/// `html` con los bloques de código de un lenguaje conocido coloreados. Los de un lenguaje
/// desconocido, y los Mermaid que no se pudieron dibujar, quedan como están. Sin bloques con
/// lenguaje devuelve el mismo texto sin cargar nada.
export async function highlightCode(html: string): Promise<string> {
  if (!html.match(BLOCK)) return html;
  const hljs = await loadHljs();
  return html.replace(BLOCK, (block, preAttrs: string, lang: string, codeAttrs: string, body: string) => {
    if (!hljs.getLanguage(lang)) return block;
    const value = highlightOne(hljs, lang, body);
    return `<pre${preAttrs}><code class="language-${lang} hljs"${codeAttrs}>${value}</code></pre>`;
  });
}
