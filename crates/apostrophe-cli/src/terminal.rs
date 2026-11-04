//! Lo que la CLI necesita saber de la terminal: si la salida va a una (y entonces si se
//! pueden usar colores) y cuantas columnas tiene. Solo lo usa `view`; el resto de los
//! comandos escribe lo mismo en cualquier destino.

/// Destino de stdout.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Terminal {
    /// Se pueden usar secuencias ANSI de color.
    pub color: bool,
    /// Columnas de la terminal; `None` si la salida no va a una (pipe o archivo).
    pub width: Option<usize>,
}

impl Terminal {
    /// La terminal real del proceso. Sin color si stdout no es una terminal, si la variable
    /// `NO_COLOR` esta definida (https://no-color.org) o si la consola no acepta ANSI.
    pub fn detect() -> Self {
        use std::io::IsTerminal;
        if !std::io::stdout().is_terminal() {
            return Self::default();
        }
        let width = terminal_size::terminal_size().map(|(w, _)| usize::from(w.0));
        let no_color = std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty());
        Self {
            color: !no_color && enable_ansi(),
            width,
        }
    }
}

/// En Windows la consola clasica interpreta las secuencias ANSI solo si se lo pide el
/// programa (`ENABLE_VIRTUAL_TERMINAL_PROCESSING`). Windows Terminal ya lo trae activado,
/// pero asi tambien funciona en la consola de siempre. Si no se puede, se sale sin color.
#[cfg(windows)]
fn enable_ansi() -> bool {
    use windows_sys::Win32::System::Console::{
        GetConsoleMode, GetStdHandle, SetConsoleMode, ENABLE_VIRTUAL_TERMINAL_PROCESSING,
        STD_OUTPUT_HANDLE,
    };
    // SAFETY: llamadas a la API de consola con el handle propio de stdout y un puntero a una
    // variable local; ninguna retiene los punteros despues de volver.
    unsafe {
        let handle = GetStdHandle(STD_OUTPUT_HANDLE);
        let mut mode = 0;
        if GetConsoleMode(handle, &mut mode) == 0 {
            return false;
        }
        mode & ENABLE_VIRTUAL_TERMINAL_PROCESSING != 0
            || SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING) != 0
    }
}

#[cfg(not(windows))]
fn enable_ansi() -> bool {
    true
}
