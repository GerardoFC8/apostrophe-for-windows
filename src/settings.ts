// Preferencias persistentes sobre tauri-plugin-store (M11.1). Espejo conceptual de como
// spellcheck.ts encapsula SU responsabilidad: aqui se concentra TODO el acceso al store
// de ajustes, asi el resto del front lee/escribe preferencias por estas dos funciones y
// nunca toca el plugin directo. El back ya registro el plugin y el permiso "store:default".
//
// El estado se guarda en "settings.json" (en el directorio de datos de la app). Se abre con
// { autoSave: false } y un unico store memoizado: el flush al disco es EXPLICITO (ver
// setSetting), porque el autosave del plugin podria no dispararse a tiempo ante el cierre
// con destroy() de la ventana.
//
// Las preferencias NO son criticas: si el store falla (disco, permisos), se devuelven los
// valores por defecto y se ignora el error en silencio — el editor sigue funcionando.

import { load } from "@tauri-apps/plugin-store";
import type { Store } from "@tauri-apps/plugin-store";

// Esquema tipado y EXTENSIBLE: el tema y otras preferencias se sumaran aqui en slices
// futuros sin tocar a los consumidores.
export interface Settings {
  spellcheck: boolean;
  /// Idioma elegido a mano. `null` = seguir al sistema (ver i18n.detectLocale). Se guarda
  /// la ausencia de eleccion, no el idioma detectado: asi quien nunca toco el selector
  /// acompaña al sistema si algun dia cambia de idioma.
  language: "es" | "en" | null;
}

const DEFAULTS: Settings = {
  spellcheck: true,
  language: null,
};

// Carga perezosa y memoizada del unico store: se abre una sola vez y se reusa la misma
// Promise en llamadas concurrentes (no recargar el archivo en cada get/set).
let storePromise: Promise<Store> | undefined;

function getStore(): Promise<Store> {
  if (storePromise === undefined) {
    // `defaults` lo exige la firma del plugin (v2.4.3): es el fallback que el store usa
    // cuando una clave no existe en disco. Pasamos los MISMOS DEFAULTS para no divergir;
    // de todos modos getSetting reaplica `?? DEFAULTS[key]` como red de seguridad.
    storePromise = load("settings.json", { defaults: { ...DEFAULTS }, autoSave: false });
  }
  return storePromise;
}

// Lee una preferencia tipada. Si la clave no existe aun (o el store falla), devuelve el
// valor por defecto. El `??` cubre tanto null como undefined que pueda devolver el plugin.
export async function getSetting<K extends keyof Settings>(key: K): Promise<Settings[K]> {
  try {
    const store = await getStore();
    return (await store.get<Settings[K]>(key)) ?? DEFAULTS[key];
  } catch {
    return DEFAULTS[key]; // store no disponible: cae a los defaults (no critico)
  }
}

// Escribe una preferencia y FUERZA el flush a disco. El save() explicito es a proposito:
// con autoSave:false el plugin no persiste solo, y al cerrar la ventana con destroy() el
// autosave podria no llegar a dispararse — asi garantizamos que el cambio quede guardado.
export async function setSetting<K extends keyof Settings>(
  key: K,
  value: Settings[K],
): Promise<void> {
  try {
    const store = await getStore();
    await store.set(key, value);
    await store.save();
  } catch {
    /* persistir preferencias no es critico: si falla, se ignora en silencio */
  }
}
