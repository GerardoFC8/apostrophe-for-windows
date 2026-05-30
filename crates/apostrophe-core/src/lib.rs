//! Nucleo de dominio de Apostrophe (arquitectura hexagonal, ADR-0001).
//!
//! Codigo PURO: no conoce Tauri, ni la WebView, ni CodeMirror. Se testea sin
//! linkear WebKitGTK, por eso vive en un crate aparte del adaptador `src-tauri`
//! (el ciclo TDD rojo-verde-refactor debe ser instantaneo).
//!
//! Modulos del nucleo (ver ADR-0001 "Que va en el NUCLEO Rust"):
//! - `stats`: contador de estadisticas de texto (PRIMER objetivo de TDD).
//! - (futuro) parser de markup (~22 regex), orquestador de conversion,
//!   politica de seguridad de preview, document store, spellcheck.

pub mod markup;
pub mod stats;
