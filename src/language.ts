// --- M15: idioma ---
// Vuelca el catalogo de i18n.ts sobre el DOM y arma el selector de idioma. Cambiar de idioma
// repinta todo lo que depende del catalogo (DOM estatico, menus generados por JS, titlebar y
// contadores), por eso este modulo orquesta a los demas en vez de al reves.
import { setSetting, getSetting } from "./settings";
import { LOCALES, LOCALE_LABEL, getLocale, setLocale, t, type Locale } from "./i18n";
import { applyTheme, buildThemeMenu, CHECK_ICON, type Theme } from "./theme";
import { closeAppMenu } from "./menu";
import { refreshTitlebar } from "./document-session";
import { updateStats } from "./stats";

const langItems = document.querySelector<HTMLElement>("#lang-items")!;

// Vuelca el catalogo sobre el DOM estatico. Cada nodo declara su clave en index.html:
// data-i18n (contenido), data-i18n-title y data-i18n-aria-label (atributos). Se llama al
// arrancar y en cada cambio de idioma, asi el repintado es uno solo y no queda texto viejo.
function applyTranslations(root: ParentNode = document): void {
  root.querySelectorAll<HTMLElement>("[data-i18n]").forEach((el) => {
    el.textContent = t(el.dataset.i18n as Parameters<typeof t>[0]);
  });
  root.querySelectorAll<HTMLElement>("[data-i18n-title]").forEach((el) => {
    el.title = t(el.dataset.i18nTitle as Parameters<typeof t>[0]);
  });
  root.querySelectorAll<HTMLElement>("[data-i18n-aria-label]").forEach((el) => {
    el.setAttribute("aria-label", t(el.dataset.i18nAriaLabel as Parameters<typeof t>[0]));
  });
}

// Los nombres de idioma NO pasan por el catalogo: cada uno va escrito en su propio idioma.
function buildLanguageMenu(): void {
  langItems.innerHTML = LOCALES.map(
    (locale) =>
      `<button class="theme-item" type="button" role="menuitemradio" data-locale="${locale}" aria-checked="false">` +
      `<span>${LOCALE_LABEL[locale]}</span>${CHECK_ICON}</button>`
  ).join("");
  langItems.querySelectorAll<HTMLButtonElement>(".theme-item").forEach((item) => {
    item.addEventListener("click", () => {
      void applyLocale(item.dataset.locale as Locale, true);
      closeAppMenu();
    });
  });
}

// Cambia el idioma y repinta TODO lo que depende del catalogo: el DOM estatico, los dos
// menus (que se generan por JS) y los contadores. `persist` distingue la eleccion del
// usuario del arranque: al arrancar no se escribe la preferencia, para no fijar un idioma
// que en realidad venia del sistema.
async function applyLocale(locale: Locale, persist: boolean): Promise<void> {
  setLocale(locale);
  applyTranslations();
  buildThemeMenu();
  buildLanguageMenu();
  // El tema activo ya vive en el dataset de <html>: reaplicarlo actualiza la etiqueta del
  // trigger y el check del item, que acaban de reconstruirse con el catalogo nuevo.
  applyTheme((document.documentElement.dataset.theme as Theme) ?? "light");
  markActiveLocale();
  refreshTitlebar();
  void updateStats();
  if (persist) await setSetting("language", locale);
}

function markActiveLocale(): void {
  langItems.querySelectorAll<HTMLElement>(".theme-item").forEach((item) => {
    item.setAttribute("aria-checked", String(item.dataset.locale === getLocale()));
  });
}

// Arranque del idioma. El idioma ya quedo fijado antes (el del sistema, ver main.ts); aca
// solo se pinta y, si hace falta, se aplica la preferencia guardada.
export function initLanguage(): void {
  applyTranslations();
  buildLanguageMenu();
  markActiveLocale();
  // Preferencia guardada: solo repinta si el usuario habia elegido un idioma distinto al del
  // sistema. `persist: false` — al arrancar no se reescribe la preferencia, para no fijar como
  // eleccion manual algo que en realidad vino del sistema.
  void (async () => {
    const saved = await getSetting("language");
    if (saved && saved !== getLocale()) await applyLocale(saved, false);
  })();
}
