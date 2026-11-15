// Teclado del dock (role=toolbar): las flechas izquierda/derecha, Inicio y Fin pasan el foco
// entre sus controles, como pide el patrón ARIA "toolbar". Tab sigue entrando y saliendo
// normalmente. Los menús que abre el dock (tema) atienden sus propias teclas.
const dock = document.querySelector<HTMLElement>("#dock")!;

function controls(): HTMLElement[] {
  return Array.from(dock.querySelectorAll<HTMLElement>("button, [tabindex='0']")).filter(
    (el) => !el.closest("[role='menu']") && el.offsetParent !== null,
  );
}

export function initDock(): void {
  dock.addEventListener("keydown", (e) => {
    if ((e.target as Element).closest("[role='menu']")) return;
    const items = controls();
    const index = items.indexOf(document.activeElement as HTMLElement);
    if (index < 0) return;
    let target: HTMLElement | undefined;
    if (e.key === "ArrowRight") target = items[(index + 1) % items.length];
    else if (e.key === "ArrowLeft") target = items[(index - 1 + items.length) % items.length];
    else if (e.key === "Home") target = items[0];
    else if (e.key === "End") target = items[items.length - 1];
    else return;
    e.preventDefault();
    target?.focus();
  });
}
