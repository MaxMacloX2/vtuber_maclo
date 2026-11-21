# Contexto y Reglas del Proyecto: `vtuber_maclo`

`vtuber_maclo` es un clon personalizado y hecho a la medida de **Veadotube Mini** (aplicación de PNGTuber) escrito en **Rust**.

## Visión del Producto
El objetivo principal es crear un avatar PNG interactivo, ligero, modular y personalizable para usar con OBS / streaming:
1. **Reactividad al micrófono**: Captura de audio en tiempo real (`cpal`) con detección de voz, umbrales, tiempos de retención (*hold time*), suavizado y futuras mejoras de audio.
2. **Detección de teclas (*hotkeys*)**: Detección de teclas (tanto dentro de la ventana como atajos globales cuando proceda) para cambiar entre distintos PNG, expresiones, estados o perfiles del modelo.
3. **Sistema modular de PNG y estados**: Gestión extensible de sprites/expresiones (por ejemplo: `idle`, `talk`, expresiones alternas, parpadeo) sin acoplarlos con condicionales rígidos en `main.rs`.
4. **Animaciones especiales del modelo**: Ejecución de animaciones propias (rebote, *squash & stretch*, sacudida, parpadeo, transiciones suaves, etc.) independientes de la tasa de refresco (usando *delta time* `dt`).

## Reglas Obligatorias de Arquitectura y Código

### 1. Arquitectura Modular y Reutilizable (DRY)
- **Separación por responsabilidades**: Evita concentrar la lógica en `src/main.rs`. Organiza el proyecto en módulos claros y enfocados (por ejemplo: `audio`, `input`, `avatar`, `animation`, `ui`, `config`). `main.rs` debe limitarse a inicializar la aplicación y conectar los módulos.
- **Componentes reutilizables**: Si una lógica puede servir en más de un lugar (curvas de animación, temporizadores de retención, carga/caché de texturas PNG, mapeo de teclas a acciones, widgets de UI), extráela en un *helper*, *struct* o *trait* reutilizable para no implementarla dos veces.
- **Funciones pequeñas y enfocadas**: Prefiere funciones cortas con retorno temprano y bajo nivel de anidación.

### 2. Comentarios Explicativos Obligatorios (en español)
- **Cada módulo** debe iniciar con un comentario `//!` explicando qué responsabilidad cumple dentro del proyecto.
- **Cada `struct`, `enum`, `trait` y función clave** debe tener documentación `///` explicando qué hace, qué representan sus campos o parámetros y cualquier invariante importante.
- **Dentro de las funciones**, agrega comentarios `//` claros y concisos que expliquen qué hace cada bloque de código relevante y **por qué** se toma esa decisión (especialmente en matemáticas de audio, animaciones, concurrencia o manejo de ventana).

### 3. Buenas Prácticas de Rust
- **Manejo seguro de errores**: Evita `.unwrap()` y `.expect()` en rutas de ejecución normal (audio, lectura de archivos, entrada de teclado, UI). Usa `Result`, `Option` o enums de error claros y degradación elegante (por ejemplo, si falla el micrófono, informar en la UI en lugar de cerrar la app).
- **Concurrencia segura y UI no bloqueante**: Nunca bloquees el hilo de `eframe`/`egui` con E/S pesada o *locks* largos. Comunica hilos de audio/teclado hacia la UI con primitivas seguras (`Arc`, `Atomic*`, canales acotados).
- **Animaciones independientes de FPS**: Toda física o animación debe multiplicarse/integrarse con `dt` (`ctx.input(|i| i.stable_dt)`), nunca con constantes fijas por frame.
- **Cero `unsafe` innecesario**: No uses `unsafe` salvo que sea estrictamente indispensable por FFI y esté justificado en comentarios.
- **Verificación continua**: Antes de dar por terminado un cambio en Rust, valida con:
  - `cargo fmt --check`
  - `cargo check`
  - `cargo clippy --all-targets`
  - `cargo test`

## Equipo de Agentes del Proyecto
Este repositorio incluye 3 agentes especializados en `.agents/agents/`:
1. **`planificador`** *(solo lectura)*: Lee el proyecto y diseña la arquitectura y el plan paso a paso antes de codificar.
2. **`implementador`**: Convierte el plan en código Rust modular, reutilizable, bien comentado y verificado con `cargo`.
3. **`revisor-bugs`**: Audita el código y los cambios en busca de bugs, errores lógicos, condiciones de carrera o duplicaciones sin modificar archivos.

