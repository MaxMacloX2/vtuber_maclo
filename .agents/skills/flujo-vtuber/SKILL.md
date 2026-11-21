---
name: flujo-vtuber
description: >-
  Orquesta el flujo completo de desarrollo en vtuber_maclo usando los 3 agentes
  especializados: planificador (solo lectura), implementador (código modular Rust)
  y revisor-bugs (auditoría de errores lógicos). Úsalo cuando el usuario pida
  planificar, implementar y revisar una nueva característica o refactorización.
---

# Flujo de Trabajo con los 3 Agentes (`vtuber_maclo`)

Cuando el usuario pida desarrollar una funcionalidad nueva, refactorizar el proyecto o usar el equipo de agentes en `vtuber_maclo`, coordina a los tres agentes en orden usando `invoke_subagent`:

## Paso 1: Planificación (`TypeName: "planificador"`)
- Invoca a `planificador` (de solo lectura) describiendo el objetivo del usuario y los archivos actuales relevantes.
- Espera su plan modular, reutilizable y con contrato técnico en Rust.
- Si el cambio afecta la arquitectura principal o introduce nuevas dependencias en `Cargo.toml`, muestra un resumen breve al usuario para confirmar antes de implementar (o continúa si el usuario pidió ejecución completa de una vez).

## Paso 2: Implementación (`TypeName: "implementador"`)
- Invoca a `implementador` pasándole el plan exacto aprobado por `planificador` y recordándole:
  - Mantener el código modular y reutilizable (sin duplicar lógica).
  - Documentar con comentarios claros en español (`//!`, `///`, `//`) qué hace cada código.
  - Seguir buenas prácticas de Rust y validar con `cargo fmt --check`, `cargo check`, `cargo clippy --all-targets` y `cargo test`.

## Paso 3: Caza de Bugs y Auditoría (`TypeName: "revisor-bugs"`)
- Una vez que `implementador` termine, invoca a `revisor-bugs` indicándole qué archivos se crearon o modificaron para que busque errores lógicos, condiciones de carrera, fallos en audio/teclado/animaciones o falta de comentarios.
- Si `revisor-bugs` reporta problemas **Críticos** o **Altos** (o incumplimientos claros de modularidad/comentarios), vuelve a invocar a `implementador` con la lista de correcciones y luego confirma con `revisor-bugs` o con los checks de `cargo`.

## Paso 4: Entrega al Usuario
- Presenta un resumen claro de:
  1. La arquitectura implementada y dónde quedó cada módulo.
  2. Qué revisó `revisor-bugs` y si quedó algún punto pendiente.
  3. El resultado de `cargo check`, `cargo clippy` y `cargo test`.

