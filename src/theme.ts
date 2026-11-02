// Selector de tema (light / dark / sepia): trigger de la titlebar + popover con un item por
// tema. El tema vive en data-theme de <html>; el editor y el chrome lo siguen solos via CSS
// vars, y el preview se re-tematiza con refreshPreviewTheme.
import { refreshPreviewTheme } from "./preview";
import { t } from "./i18n";

const themeTrigger = document.querySelector<HTMLButtonElement>("#theme-trigger")!;
const themeTriggerIcon = document.querySelector<HTMLElement>("#theme-trigger-icon")!;
const themeTriggerLabel = document.querySelector<HTMLElement>("#theme-trigger-label")!;
const themeMenu = document.querySelector<HTMLElement>("#theme-menu")!;

// --- M5: temas (light / dark / sepia) ---
const THEMES = ["light", "dark", "sepia"] as const;
export type Theme = (typeof THEMES)[number];
// Las etiquetas de tema se resuelven en cada llamada (no en una constante): al cambiar de
// idioma el menu se reconstruye y tiene que leer el catalogo nuevo.
const themeLabel = (theme: Theme): string =>
  ({ light: t("theme.light"), dark: t("theme.dark"), sepia: t("theme.sepia") })[theme];

// Iconos SVG (stroke = currentColor, heredan el color del tema). Sol / luna / libro.
const ICON: Record<Theme, string> = {
  light:
    '<svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"><circle cx="8" cy="8" r="3"/><path d="M8 1.2V2.8M8 13.2V14.8M1.2 8H2.8M13.2 8H14.8M3.4 3.4 4.5 4.5M11.5 11.5 12.6 12.6M12.6 3.4 11.5 4.5M4.5 11.5 3.4 12.6"/></svg>',
  dark:
    '<svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round"><path d="M13.4 9.3A5.6 5.6 0 0 1 6.7 2.6 5.6 5.6 0 1 0 13.4 9.3Z"/></svg>',
  sepia:
    '<svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round"><path d="M8 3.6C6.6 2.7 4.2 2.7 2.7 3.2V12c1.5-.5 3.9-.5 5.3.4M8 3.6c1.4-.9 3.8-.9 5.3-.4V12c-1.5-.5-3.9-.5-5.3.4M8 3.6V12.4"/></svg>',
};

export function getInitialTheme(): Theme {
  const saved = localStorage.getItem("theme");
  if (saved === "light" || saved === "dark" || saved === "sepia") return saved;
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

export function applyTheme(theme: Theme): void {
  document.documentElement.dataset.theme = theme;
  localStorage.setItem("theme", theme);
  // Reflejar en el trigger del selector y marcar el item activo del menu.
  themeTriggerIcon.innerHTML = ICON[theme];
  themeTriggerLabel.textContent = themeLabel(theme);
  themeMenu.querySelectorAll<HTMLElement>(".theme-item").forEach((item) => {
    item.setAttribute("aria-checked", String(item.dataset.theme === theme));
  });
  // Re-tematizar el preview ya renderizado (editor/chrome se actualizan solos via vars).
  refreshPreviewTheme();
}

// Check del item activo, compartido por el menu de temas y el de idiomas.
export const CHECK_ICON =
  '<svg class="check" width="12" height="12" viewBox="0 0 12 12" aria-hidden="true"><path d="M2.5 6.4 L5 8.9 L9.5 3.4" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/></svg>';

// Construye el menu del selector: un item por tema (icono + label + check del activo).
export function buildThemeMenu(): void {
  themeMenu.innerHTML = THEMES.map(
    (theme) =>
      `<button class="theme-item" type="button" role="menuitemradio" data-theme="${theme}" aria-checked="false">` +
      `<span class="theme-item-icon">${ICON[theme]}</span><span>${themeLabel(theme)}</span>` +
      CHECK_ICON +
      "</button>"
  ).join("");
  themeMenu.querySelectorAll<HTMLButtonElement>(".theme-item").forEach((item) => {
    item.addEventListener("click", () => {
      applyTheme(item.dataset.theme as Theme);
      closeThemeMenu();
    });
  });
}

function openThemeMenu(): void {
  themeMenu.hidden = false;
  themeTrigger.setAttribute("aria-expanded", "true");
}
export function closeThemeMenu(): void {
  themeMenu.hidden = true;
  themeTrigger.setAttribute("aria-expanded", "false");
}

// ¿Esta abierto el popover de temas? (lo consulta el Esc de pantalla completa).
export function isThemeMenuOpen(): boolean {
  return !themeMenu.hidden;
}

export function initThemeMenu(): void {
  themeTrigger.addEventListener("click", (e) => {
    e.stopPropagation(); // que no llegue al cierre-por-click-afuera de document
    if (themeMenu.hidden) openThemeMenu();
    else closeThemeMenu();
  });
  document.addEventListener("click", () => closeThemeMenu()); // click afuera cierra
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") closeThemeMenu();
  });
}
