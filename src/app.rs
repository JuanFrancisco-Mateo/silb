//! UI de Whispr Open con Iced: dictado por turnos, transcript acumulado,
//! limpieza de muletillas, diccionario personal, selector de modelo e
//! inyección en la app activa.
//!
//! # Mapa de módulos (refactor de organización; cero cambio de conducta)
//!
//! Este fichero se queda con el estado y el esqueleto de la app: el struct
//! `WhisprApp`, `new`, el despachador `update` (un `match` que delega en un
//! manejador por familia de mensajes), `subscription`, `theme`, `view` y los
//! helpers pequeños de estado. El resto de la UI vive en `src/ui/`:
//!
//! - `ui::settings`: las vistas del panel de Ajustes (`settings_view`,
//!   `settings_tab_bar`, `settings_section_*`) y los helpers de paleta.
//! - `ui::settings_handlers`: manejadores de Ajustes (`handle_note`,
//!   `handle_lists`, `handle_dictation_engine`, `handle_appearance_settings`,
//!   `handle_palette_settings`, `handle_corrector_settings`).
//! - `ui::recording`: grabación y transcripción
//!   (`handle_recording_control`, `handle_transcription_done`,
//!   `handle_dictation_settings`).
//! - `ui::poll`: el tick periódico (`handle_poll`).
//! - `ui::bubble`: los eventos de la burbuja flotante
//!   (`handle_bubble_events`).
//! - `ui::window_tray`: ventana, bandeja y arranque
//!   (`handle_window_and_tray`).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;

use global_hotkey::GlobalHotKeyEvent;
use iced::widget::text::Shaping;
use iced::widget::{
    button, column, container, pick_list, progress_bar, row, scrollable, text, text_editor,
    Button, Column, Space, Text,
};
use iced::{window, Background, Color, Element, Font, Length, Point, Subscription, Task, Theme};

use crate::audio::AudioCapture;
use crate::config::{
    BubbleCorner, BubbleHideTimeout, BubbleSize, BubbleStyle, Config, CorrectionProvider,
    FontSize, ThemeMode,
};
use crate::domain::cleanup::{clean_transcript_with, FillerWords};
use crate::domain::correction::CorrectionSession;
use crate::domain::dictionary::{apply_dictionary, DictionaryEntry};
use crate::hotkey::HotkeyManager;
use crate::inject::{inject_text_into_active_app, InjectOutcome};
use crate::theme::{self, UiPalette};
use crate::transcribe::Model;

#[derive(Debug, Clone, PartialEq)]
/// Estado global de la app tal como lo lee la UI (hero, burbuja, bandeja).
/// Un solo enum evita combinaciones imposibles («grabando y descargando»).
pub enum Status {
    Idle,
    Recording,
    Transcribing,
    /// El LLM corrector está trabajando sobre el último turno dictado
    /// (docs/CORRECCION-VOZ.md §4.4: estado ámbar como Transcribing).
    Correcting,
    Downloading {
        model: Model,
        downloaded: u64,
        total: u64,
    },
    Error(String),
}

/// Fase de arranque de la app (requisito UX 09/09/2026: «la app levanta todo
/// por sí misma»). Mientras no está Ready la vista principal muestra SOLO la
/// pantalla de carga (spinner + texto) y el dictado queda bloqueado: la
/// primera pulsación del atajo no debe encontrarse el modelo de voz aún en
/// disco.
#[derive(Debug, Clone, PartialEq)]
pub enum StartupPhase {
    /// Whisper (y luego el server LLM si procede) cargándose en background.
    Loading,
    /// Todo listo: dictado disponible.
    Ready,
    /// Fallo bloqueante de whisper (modelo ausente y descarga fallida, o
    /// contexto que no carga): la app no puede transcribir. `error` es el
    /// motivo legible. «Reintentar» relanza el arranque completo.
    Failed { error: String },
}

/// Estado del corrector LLM local tras el auto-inicio (o su ausencia). La
/// corrección IA solo se enruta cuando está `Ready`; `Disabled` muestra el
/// motivo en la UI (nunca silencioso) pero el dictado sigue funcionando.
#[derive(Debug, Clone, PartialEq)]
pub enum LlmStatus {
    /// Aún no comprobado (arranque en curso, o proveedor cloud/custom que
    /// no necesita server local).
    Unknown,
    /// Server LLM respondiendo: ya corría antes de abrir la app o lo lanzó
    /// ella misma. `launched` distingue el caso (log).
    Ready { launched: bool },
    /// Corrección IA desactivada en runtime con motivo visible. Razón vacía
    /// = desactivada por el usuario en Ajustes (sin banner de error).
    Disabled(String),
}

/// Eventos del hilo de arranque en background (prerroll whisper + server
/// LLM). Se entregan por `startup_rx` y se procesan en el Poll (patrón
/// transcribe_rx/llm_rx: nunca bloquear el bucle de la UI).
#[derive(Debug, Clone)]
pub enum StartupEvent {
    /// Progreso de la descarga del modelo whisper (0,0 = descarga arrancada;
    /// con `total=0` aún no se conoce el tamaño).
    WhisperProgress { downloaded: u64, total: u64 },
    /// El modelo whisper está descargado y cargado en el cache de contexto.
    WhisperReady,
    /// La carga/descarga de whisper falló: la app no puede transcribir.
    WhisperFailed(String),
    /// La fase LLM comienza (verificando server / lanzándolo).
    LlmStarting,
    /// Server LLM listo (ya corría o lo lanzó la app) — arranque terminado.
    LlmReady { launched: bool },
    /// Server LLM no disponible (o corrección off): arranque terminado con
    /// la corrección IA desactivada. Razón vacía = el usuario la apagó.
    LlmDisabled(String),
}

/// Lectura visual del estado del micrófono (feedback doble: color + texto).
struct MicUi {
    color: Color,
    hint_color: Color,
    label: String,
    hint: String,
}

/// La instrucción hablada que se está enviando al LLM corrector. Si la
/// llamada falla o el diff no aplica, se reinserta como dictado normal
/// (spec §3.3.1: nunca perder texto). `target` es el turno sobre el que se
/// pidió la corrección (capturado al despachar: un turno nuevo dictado
/// mientras el LLM trabaja no debe desviar el diff a otro texto).
///
/// Desde t_78d9e1fa hay dos modos:
///
/// - `CrossTurn`: el LLM devuelve un `Diff {find, replace}` que se aplica
///   sobre `target`. Modo histórico (spec §3.2-3.4).
/// - `InTurn`: el LLM devuelve el texto FINAL corregido del turno (sin
///   meta-lengua). `target` es el turno completo enviado al LLM; el
///   resultado REEMPLAZA ese turno entero. Modo principal desde el
///   10/09/2026 (spec §3.5).
///
/// `instruction` solo se usa en `CrossTurn`; en `InTurn` el input que
/// recibió el LLM es exactamente `target`. Se conserva el campo por
/// uniformidad (los logs y el fallback de error son más fáciles).
pub(crate) enum LlmMode {
    CrossTurn,
    InTurn,
}

pub(crate) struct LlmPending {
    pub(crate) mode: LlmMode,
    pub(crate) instruction: String,
    /// Último turno dictado que la instrucción pretende corregir
    /// (cross-turn) o turno completo enviado al LLM (in-turn).
    pub(crate) target: String,
    /// El turno venía de la burbuja (inyección forzada al soltar).
    pub(crate) force_inject: bool,
}

/// Resultado del LLM que viaja por el canal `llm_rx`. El despacho mete
/// el resultado dentro del enum correspondiente; la UI lo consume en
/// `handle_llm_correction` o `handle_llm_redaction` según `mode`.
///
/// `pub` porque aparece dentro de `Message::CorrectionDone(Result<LlmResult, String>)`,
/// que sí es público (variante del enum usado por la API de mensajes
/// de iced). El enum no se expone al usuario final; solo es la firma
/// del canal interno entre el hilo del LLM y el bucle de UI.
#[derive(Debug, Clone)]
pub enum LlmResult {
    Diff(crate::domain::corrective::Diff),
    Redacted(String),
}

pub(crate) struct PendingDiff {
    diff: crate::domain::corrective::Diff,
    target: String,
    force_inject: bool,
}

/// Todos los eventos que la UI sabe procesar: pulsaciones de botones,
/// resultados de hilos (transcripción, corrección), eventos del sistema
/// (atajo, bandeja, ventanas) y el tick del Poll. Es la «bandeja de
/// entrada» única del bucle de eventos de Iced.
#[derive(Debug, Clone)]
pub enum Message {
    Toggle,
    StartRecording,
    StopRecording,
    /// El stream de audio murió a mitad de grabación (micro desconectado,
    /// driver caído). Llega desde el Poll al drenar el canal de errores de
    /// `AudioCapture`: el turno se corta YA y se transcribe lo grabado hasta
    /// ese momento (nunca se pierde) con un aviso visible.
    AudioDeviceLost(String),
    TranscriptionDone(Result<String, String>),
    ModelSelected(Model),
    ToggleCleanup(bool),
    ToggleAutoInject(bool),
    /// Corrección IA (otro modelo) activada/desactivada — Ajustes.
    ToggleAiCorrection(bool),
    /// Proveedor de corrección: local por defecto, cloud o endpoint propio.
    CorrectionProviderChanged(CorrectionProvider),
    /// Endpoint del LLM corrector (base_url OpenAI-compatible). Vacío
    /// en la UI cae al default `http://127.0.0.1:8081/v1`.
    LlmEndpointChanged(String),
    /// Modelo del corrector. Vacío en la UI cae al default
    /// `ornith-1.0-9b`.
    LlmModelChanged(String),
    LlmApiKeyChanged(String),
    /// Timeout HTTP en segundos (1..=120). Sale del slider; el getter
    /// lo pasa a `LlmConfig::timeout_secs` y `ureq::timeout`.
    LlmTimeoutChanged(u32),
    /// Ruta del ejecutable del server LLM (`llama-server.exe`) tecleada en
    /// Ajustes. Vacío = autodetección por rutas comunes.
    LlmServerPathChanged(String),
    /// Abre el diálogo nativo «Examinar…» para elegir `llama-server.exe`.
    LlmServerPathBrowse,
    /// Resultado del diálogo de archivo (None = cancelado). Persiste la
    /// ruta elegida en config.json.
    LlmServerPathPicked(Option<String>),
    /// Vuelve a la autodetección (borra la ruta explícita de config.json).
    LlmServerPathReset,
    /// Umbral del clasificador (0..=100). 0 = clasificador pasivo
    /// (atajo equivalente al toggle maestro apagado); 100 =
    /// imposible que enrute; default 50 = conducta previa.
    LlmThresholdChanged(u32),
    /// Modo «confirmar antes de aplicar» (spec §4.2). Cuando está on,
    /// al volver del LLM el diff se muestra con botones «Aplicar» /
    /// «Descartar» en vez de aplicarse automáticamente.
    LlmConfirmToggled(bool),
    /// Aplica el diff pendiente que se mostró tras una corrección IA
    /// cuando `ai_correction_confirm` está activo.
    ApplyPendingDiff,
    /// Descarta el diff pendiente (reinserta la instrucción hablada
    /// como dictado normal, spec §3.3.1).
    DiscardPendingDiff,
    /// Resultado del LLM corrector. Llega por `Message::CorrectionDone`
    /// con un `LlmResult` que indica el modo:
    ///   - `LlmResult::Diff(diff)` (cross-turn, spec §3.2-3.4): se aplica
    ///     el diff sobre `target` (el último turno).
    ///   - `LlmResult::Redacted(text)` (in-turn, spec §3.5): el LLM
    ///     devuelve el texto final corregido del turno (sin meta-lengua);
    ///     sustituye a `target` por completo.
    ///
    /// El despacho del corrector decide el modo mirando
    /// `self.llm_pending.mode`; el result type no necesita su propio
    /// discriminador de modo (lo que viaja es la forma del output, no
    /// la intención del clasificador). Esto simplifica el Poll y el
    /// manejo de errores.
    CorrectionDone(Result<LlmResult, String>),
    /// Deshacer la última corrección IA aplicada (un nivel).
    UndoLastCorrection,
    // ── Nota editable (requisito R3) ────────────────────────────────────
    /// Acción del editor multilínea del transcript (teclear, borrar,
    /// pegar, mover el cursor…). Se aplica al `Content` y se sincroniza
    /// `transcript`/`last_turn`: editar a mano y seguir dictando no se
    /// pisan.
    TranscriptEdited(text_editor::Action),
    /// Primer clic en «Limpiar»: ARMA la confirmación («Sí, borrar» /
    /// «No»). Borrar la nota es destructivo y debe ser explícito.
    ClearPressed,
    /// Confirmación del borrado (segundo clic). Borra con backup para
    /// «Deshacer borrado».
    ClearTranscript,
    /// Restaura la nota borrada desde el backup del borrado.
    UndoClearTranscript,
    CopyTranscript,
    InjectLast,
    AddEntry,
    EntryFromChanged(String),
    EntryToChanged(String),
    RemoveEntry(usize),
    // ── Muletillas (Ajustes) ──────────────────────────────────────────────
    /// Añade la muletilla escrita en `filler_input` a la categoría
    /// seleccionada (botón «Añadir» o Enter en el input).
    AddFiller,
    FillerInputChanged(String),
    FillerCategorySelected(FillerCategory),
    RemoveFiller {
        category: FillerCategory,
        index: usize,
    },
    /// Vuelve a las listas por defecto de `cleanup.rs` (config.fillers = None).
    ResetFillers,
    // ── Diccionario: importar/exportar (backup/restore a JSON) ────────────
    /// Exporta el diccionario a `%APPDATA%/whispr-open/dictionary.json`.
    ExportDictionary,
    /// Importa el diccionario desde ese fichero (reemplaza la lista actual).
    ImportDictionary,
    InputDeviceSelected(String),
    /// Idioma de transcripción seleccionado en Ajustes. Valor crudo:
    /// `"auto"` → autodetección; cualquier otro → BCP-47 (`"es"`, `"en"`,
    /// …). Se persiste tal cual en `config.language`; el getter decide
    /// cómo traducirlo a `whisper_full_params`.
    LanguageSelected(String),
    /// Beam size elegido por el usuario (1..=8). Se persiste en
    /// `config.beam_size`; el getter clampea al rango válido.
    BeamSizeChanged(u32),
    /// Hilos del decoder elegidos por el usuario (1..=32). Persistido
    /// en `config.n_threads`; el getter clampea.
    ThreadsChanged(u32),
    /// Modo de tema elegido en Ajustes (el header cicla el mismo estado).
    ThemeModeChanged(ThemeMode),
    /// Tamaño de fuente global elegido en Ajustes (se aplica a toda la vista).
    FontSizeChanged(FontSize),
    /// Tamaño del círculo de la burbuja (resize EN CALIENTE vía window::resize).
    BubbleSizeChanged(BubbleSize),
    /// Esquina de anclaje de la burbuja (mover en caliente vía winapi).
    BubbleCornerChanged(BubbleCorner),
    /// Opacidad de la burbuja en % (slider 20–100, alpha por-pixel).
    BubbleOpacityChanged(u32),
    BubbleStyleChanged(BubbleStyle),
    /// Auto-ocultar la burbuja tras N segundos sin dictar (Nunca = desactivado).
    BubbleHideTimeoutChanged(BubbleHideTimeout),
    ToggleTheme,
    ToggleSettings,
    /// Abre/cierra el panel «Ajustes avanzados» dentro de Ajustes (las
    /// opciones técnicas quedan ocultas por defecto; el estado expandido o
    /// colapsado se persiste en `config.advanced_settings_open`).
    ///
    /// NOTA (R7 — Personalización I): tras el refactor por pestañas el panel
    /// «Ajustes avanzados» deja de existir como bloque colapsable dentro del
    /// panel. Lo técnico del corrector pasa a la pestaña «Corrección IA» y lo
    /// técnico del motor de voz a «Dictado». El mensaje se mantiene por
    /// compatibilidad hacia atrás pero ya no se envía desde la UI.
    ToggleAdvancedSettings,
    /// Cambia la pestaña activa dentro del panel de Ajustes (R7). Solo
    /// relevante mientras `settings_open` sea true.
    SettingsSectionSelected(SettingsSection),
    /// Cambia la SECCIÓN de la ventana principal (H2 — UI 24/09/2026):
    /// `Dictado` (hero del micro + nota) o `Ajustes` (panel completo). El
    /// estado sigue siendo `settings_open`: esta sección es solo su lectura
    /// desde la vista, para que la ventana tenga pestañas como Ajustes en vez
    /// de una columna de cosas.
    WindowSectionSelected(WindowSection),
    /// Abre en el Explorador la carpeta donde vive el registro
    /// (`%APPDATA%/whispr-open/`). Pedido del usuario: «me pierdo buscando el
    /// log» (H2 del QA 24/09/2026).
    OpenLogFolder,
    Poll,
    // ── Personalización II (R7): paleta editable ─────────────────────────
    /// Texto del campo hex de un campo concreto de la paleta. Cada tecla
    /// actualiza la previsualización (en el swatch + el aviso de contraste)
    /// pero NO persiste hasta que el usuario pulsa «Aplicar» o edita otro
    /// campo (debounce, como el resto de inputs de Ajustes).
    PaletteFieldChanged(theme::PaletteField, String),
    /// «Aplicar» de un campo concreto: persiste el valor actual del input
    /// (parseo robusto, hex inválido = se ignora).
    PaletteFieldApply(theme::PaletteField),
    /// «Restaurar» un campo concreto: lo devuelve a `None` (default del
    /// modo activo).
    PaletteFieldReset(theme::PaletteField),
    /// «Restaurar TODO»: borra la paleta entera (vuelve a defaults DARK/LIGHT).
    PaletteResetAll,
    /// Aplicar un preset (5 disponibles). Pasa a `Config::palette` el
    /// `PaletteConfig` correspondiente al modo activo (DARK o LIGHT según
    /// `ThemeMode` resuelto).
    PalettePresetApply(theme::PalettePreset),
    /// Exporta la paleta a un fichero JSON elegido por el usuario con
    /// `rfd::FileDialog::save_file`. El resultado (ruta o None si se
    /// canceló) vuelve como `Message::PaletteExportPicked`.
    PaletteExport,
    /// Resultado del diálogo de export. `Some(ruta)` = JSON escrito; `None`
    /// = usuario canceló (no es error).
    PaletteExportPicked(Option<std::path::PathBuf>),
    /// Importa la paleta desde un fichero JSON elegido con
    /// `rfd::FileDialog::pick_file`. El resultado vuelve como
    /// `PaletteImportPicked`. Un JSON corrupto o inválido se rechaza SIN
    /// tocar la paleta actual (la UI sigue con lo que tenía).
    PaletteImport,
    /// Resultado del diálogo de import: ruta o cancelación. La ruta se
    /// procesa en `update` para leer + parsear + fundir con la paleta actual
    /// (preservando lo que el usuario no esté reemplazando explícitamente).
    PaletteImportPicked(Option<std::path::PathBuf>),
    // ── Personalización III (R7): imágenes del usuario ────────────────
    /// Abrir el diálogo nativo para elegir la imagen de fondo de la
    /// ventana principal. `rfd::FileDialog` filtra por extensiones de
    /// imagen comunes; el resultado (ruta o None si se canceló) vuelve
    /// como `BgImagePicked`.
    BgImageBrowse,
    /// Resultado del diálogo de imagen de fondo. `Some(path)` = persistir
    /// y aplicar; `None` = usuario canceló (no es error).
    BgImagePicked(Option<String>),
    /// Vacía la ruta de la imagen de fondo (vuelve al fondo del tema).
    BgImageClear,
    /// Abrir el diálogo para elegir el sprite estático de la burbuja.
    BubbleImageBrowse,
    BubbleImagePicked(Option<String>),
    BubbleImageClear,
    /// Abrir el diálogo para elegir la carpeta con los frames de la
    /// animación de la burbuja. `rfd::FileDialog::pick_folder`.
    BubbleFramesBrowse,
    BubbleFramesPicked(Option<String>),
    BubbleFramesClear,
    /// FPS de la animación de la burbuja (1..=30). Sale del slider; el
    /// getter ya clampea.
    BubbleFpsChanged(u32),
    /// Tick del reloj de animación: avanza el frame del sprite de la
    /// burbuja. Se envía desde la subscription de `iced::time::every`
    /// solo cuando la animación está activa (carpeta de frames
    /// resuelta + FPS > 0). El Poll lo ignora si la animación está
    /// parada.
    AnimationTick,
    // ── Burbuja flotante (overlay) ────────────────────────────────────────
    /// La ventana de la burbuja ya fue creada (aplicar WS_EX_NOACTIVATE).
    BubbleOpened(window::Id),
    /// Botón izquierdo pulsado dentro de la burbuja (empieza a grabar).
    BubblePress,
    /// Botón izquierdo soltado (transcribir + escribir en la app activa).
    BubbleRelease,
    /// Botón derecho pulsado dentro de la burbuja (arma el arrastre).
    BubbleRightPress,
    /// Botón derecho soltado (fin del arrastre; clic simple = no-op).
    BubbleRightRelease,
    /// Cursor movido dentro de la burbuja (detección de arrastre derecho).
    BubbleCursorMoved(Point),
    /// Cursor salió de la burbuja sin soltar (cerrar botones pendientes).
    BubbleCursorLeft,
    /// El usuario pidió cerrar una ventana (X de la principal → a la bandeja).
    WindowCloseRequested(window::Id),
    // ── Bandeja del sistema (tray) ────────────────────────────────────────
    /// Se abrió una ventana; con su id se identifica la principal.
    WindowOpened(window::Id),
    /// «Mostrar/Ocultar ventana» pulsado en el menú de la bandeja.
    ToggleMainWindow,
    /// La fuente emoji terminó de cargarse en el FontSystem de Iced
    /// (resultado de `iced::font::load` al arrancar; `Error` es ininhabitable
    /// en iced 0.13, pero se loguea por si en el futuro se propaga algo).
    FontLoaded(Result<(), iced::font::Error>),
    // ── Arranque (prerroll whisper + server LLM) ──────────────────────────
    /// Evento del hilo de arranque en background (ver [`StartupEvent`]).
    Startup(StartupEvent),
    /// «Reintentar» desde la pantalla de error de arranque: relanza el
    /// prerroll completo (whisper → LLM).
    RetryStartup,
}

/// La aplicación completa: configuración, estado de dictado, nota editable
/// y los canales de los hilos de trabajo. Es la única estructura con
/// acceso a la UI; todo lo demás son módulos sin estado.
pub struct WhisprApp {
    pub(crate) config: Config,
    pub(crate) status: Status,
    pub(crate) transcript: String,
    pub(crate) last_turn: String,
    pub(crate) removed_fillers: Vec<String>,
    pub(crate) last_correction: Option<String>,
    pub(crate) session: CorrectionSession,
    pub(crate) audio: Option<AudioCapture>,
    pub(crate) input_devices: Vec<String>,
    pub(crate) transcribe_rx: Option<Receiver<Result<String, String>>>,
    pub(crate) hotkey_rx: crossbeam_channel::Receiver<GlobalHotKeyEvent>,
    pub(crate) hotkey: HotkeyManager,
    /// Etiqueta canónica del atajo global registrado («Ctrl+Shift+D» o el
    /// `shortcut` de config.json) para los textos de la UI.
    pub(crate) hotkey_label: String,
    /// Placeholder del editor de la nota (vive como campo porque la API
    /// del editor pide un `&str` prestado por frame).
    pub(crate) placeholder_text: String,
    /// Editor multilínea del transcript (requisito R3: la nota se puede
    /// corregir a mano). Es la vista viva del texto; `transcript` sigue
    /// siendo la fuente de verdad del pipeline (inyección, correcciones,
    /// copiado) y se sincroniza en cada acción de edición.
    pub(crate) transcript_editor: text_editor::Content,
    /// Momento del último «Copiar» con éxito: alimenta la confirmación
    /// visual breve del botón («✓ Copiado», ~2 s, requisito R3).
    pub(crate) copied_at: Option<std::time::Instant>,
    /// El botón «Limpiar» está armado esperando la confirmación explícita
    /// («Sí, borrar» / «No») — borrar la nota es destructivo y debe ser
    /// una acción deliberada (requisito R3). Caduca a los 5 s.
    pub(crate) clear_armed: bool,
    pub(crate) clear_armed_at: Option<std::time::Instant>,
    /// Backup (transcript, last_turn) del último borrado, para «Deshacer
    /// borrado» (requisito R3: reversible).
    pub(crate) cleared_backup: Option<(String, String)>,
    /// Velocidad de descarga estimada (EMA de bytes/s) para el tiempo
    /// restante del estado de descarga (requisito R4: MB + ETA, no solo %).
    pub(crate) dl_rate: Option<f64>,
    /// Última muestra de progreso (bytes, instante) para la EMA.
    pub(crate) dl_last: Option<(u64, std::time::Instant)>,
    pub(crate) entry_from: String,
    pub(crate) entry_to: String,
    /// Muletilla en curso de escritura (Ajustes → Muletillas).
    pub(crate) filler_input: String,
    /// Categoría seleccionada en el `pick_list` de muletillas.
    pub(crate) filler_category: FillerCategory,
    pub(crate) settings_open: bool,
    /// Pestaña activa dentro del panel de Ajustes (R7 — Personalización I).
    /// In-memory; NO se persiste en config.json (regla del producto: la
    /// última sección abierta solo se recuerda dentro de la sesión).
    /// Se resetea a `General` cada vez que se cierra el panel, para que
    /// al volver a abrirse el usuario aterrice en lo más común (requisito
    /// 5 de la tarea R7: «recordar la última sección solo en el estado de
    /// sesión»).
    pub(crate) settings_section: SettingsSection,
    // ── Personalización II (R7): buffers de input por campo de paleta ────────
    /// Texto en vuelo del input hex de cada campo de la paleta (lo que el
    /// usuario está tecleando, antes de pulsar «Aplicar» o pasar a otro
    /// campo). Vacío por defecto; se rellena al entrar a la sección Apariencia
    /// con el hex actual de cada campo (o vacío si está en default). La razón
    /// de vivir aquí (vs. un `State` de iced): queremos comparar contra el
    /// valor persistido para no machacar lo que el usuario está escribiendo.
    pub(crate) palette_inputs: std::collections::HashMap<theme::PaletteField, String>,
    // ── Estado de la burbuja flotante ─────────────────────────────────────
    pub(crate) bubble_id: Option<window::Id>,
    pub(crate) bubble_pressed: bool, // botón izquierdo pulsado (graba)
    pub(crate) bubble_last_pos: Point,
    /// Escritura de configuración diferida para no bloquear la UI al teclear
    /// endpoint/model/key ni al mover sliders.
    pub(crate) config_save_deadline: Option<std::time::Instant>,
    /// Botón derecho: solo arrastra la burbuja (nunca graba ni abre nada).
    pub(crate) bubble_right: crate::bubble::RightDragState,
    /// Captura del ratón + seguimiento de la posición durante el arrastre.
    pub(crate) bubble_drag: Option<crate::bubble::BubbleDrag>,
    pub(crate) pending_inject: bool, // la próxima transcripción debe inyectarse sí o sí
    /// Índice de frame actual en la animación por conmutación de la
    /// burbuja. Avanzado por `Message::AnimationTick` (enviado desde la
    /// suscripción de `iced::time::every` cuando hay frames resueltos).
    /// `0` por defecto; se reinicia al elegir una nueva carpeta de
    /// frames (ver `Message::BubbleFramesPicked`).
    pub(crate) bubble_frame_index: usize,
    // ── Corrección IA (LLM) ────────────────────────────────────────────────
    pub(crate) llm_rx: Option<Receiver<Result<LlmResult, String>>>,
    /// Instrucción en ruta al LLM (con el target capturado): si la llamada
    /// falla o el diff no aplica, se reinserta como dictado normal.
    pub(crate) llm_pending: Option<LlmPending>,
    /// Diff pendiente de confirmación. Conserva también el turno objetivo y
    /// el modo de inyección para que dictados posteriores no redirijan la
    /// corrección a otro texto.
    pub(crate) pending_diff: Option<PendingDiff>,
    /// Undo de 1 nivel: (transcript, last_turn) antes de la última corrección.
    pub(crate) undo_state: Option<(String, String)>,
    // ── Bandeja del sistema ───────────────────────────────────────────────
    pub(crate) tray: Option<crate::tray::SystemTray>,
    /// Id de la ventana principal (se descubre con `window::open_events()`:
    /// iced no lo expone al crear la ventana desde `program::run`).
    pub(crate) main_window_id: Option<window::Id>,
    /// Si la ventana principal está visible (controla el texto del menú).
    pub(crate) main_visible: bool,
    // ── Apariencia (tema auto + auto-ocultar de la burbuja) ──────────────
    /// Último estado conocido del tema del SISTEMA (AppsUseLightTheme). Se
    /// cachea aquí porque el view se redibuja ~10×/s y leer el registro en
    /// cada redibujo sería derrochador; el Poll lo refresca cada 5 s.
    pub(crate) system_dark: bool,
    /// Última vez que se refrescó `system_dark` (tema auto, cada 5 s).
    pub(crate) last_theme_check: std::time::Instant,
    /// Cuándo auto-ocultar la burbuja (Some = timer armado tras un turno).
    /// `None` = visible sin plazo (Nunca o turno sin terminar).
    pub(crate) bubble_hide_deadline: Option<std::time::Instant>,
    /// La burbuja está oculta por el auto-ocultar (evita re-enviar
    /// change_mode(Windowed) a una ventana que ya está visible).
    pub(crate) bubble_hidden: bool,
    // ── Arranque (prerroll whisper + server LLM) ─────────────────────────
    /// Fase de arranque: Loading/Ready/Failed. Mientras no esté Ready la
    /// vista principal es solo la pantalla de carga y el dictado está
    /// bloqueado.
    pub(crate) startup: StartupPhase,
    /// Estado del corrector LLM tras el auto-inicio (gate del enrutado).
    pub(crate) llm_status: LlmStatus,
    /// Canal de eventos del hilo de arranque (se drena en el Poll; None al
    /// terminar o si no hay arranque en curso).
    pub(crate) startup_rx: Option<Receiver<StartupEvent>>,
    /// Token de cancelación de la fase LLM en vuelo (toggle off en caliente):
    /// cuando el usuario apaga la corrección IA, la fase que esté lanzando o
    /// esperando al server debe abortar sin dejar modelo cargado. Cada fase
    /// nueva recibe su propio `AtomicBool` compartido con su hilo.
    pub(crate) llm_phase_cancel: Option<Arc<AtomicBool>>,
    /// Texto de la línea de estado de la pantalla de carga («Cargando modelo
    /// de voz…», «Preparando corrección IA…», …).
    pub(crate) startup_label: String,
    /// Tick de animación del spinner de carga (lo avanza el Poll a 10 Hz).
    pub(crate) startup_tick: u32,
    // ── Robustez de grabación ────────────────────────────────────────────
    /// Cuándo empezó el turno en curso (`None` = no se está grabando). El
    /// Poll lo vigila: al llegar a `audio::MAX_RECORDING_SECS` el turno se
    /// detiene solo (memoria acotada en dictados maratonianos) y el audio
    /// grabado hasta ahí se transcribe con normalidad.
    pub(crate) recording_started: Option<std::time::Instant>,
    /// El turno en curso terminó porque el micrófono murió a mitad (el
    /// mensaje de `AudioDeviceLost`). La transcripción resultante se
    /// acompaña de un aviso claro en vez de aparecer como si nada.
    pub(crate) device_lost: bool,
    /// Nota pendiente de mostrar cuando llegue la transcripción del turno
    /// (p. ej. el aviso del tope de duración). `apply_pipeline` limpia
    /// `last_correction` al procesar un turno normal, así que la nota se
    /// aplica DESPUÉS, al recibir `TranscriptionDone`.
    pub(crate) pending_turn_note: Option<String>,
}

impl WhisprApp {
    /// Construye la app: carga config, registra atajo/bandeja, abre la
    /// burbuja y lanza el hilo de arranque (descarga + preroll de whisper,
    /// server LLM si aplica). Devuelve el estado inicial y la primera
    /// `Task` de Iced (abrir la burbuja + cargar la fuente emoji).
    pub fn new() -> (Self, Task<Message>) {
        // Config persistente. Antes de nada, saneado de la ruta del server
        // LLM (t_75732630): una ruta legacy al wrapper `llama.exe` —o a un
        // fichero inexistente— se corrige al `llama-server.exe` nativo y se
        // PERSISTE. Con el wrapper, el proceso lanzado por la app no casaba
        // con la validación por nombre de imagen al salir y quedaba huérfano
        // en VRAM.
        let mut config = Config::load();
        let _ = config.heal_llama_server_path();
        // H4 (QA 24/09/2026): las rutas de personalización que ya no apuntan a
        // nada se quitan del config al arrancar (avisando en el log). El
        // usuario que vacía Descargas deja de arrastrar una ruta muerta; una
        // config vieja con una ruta externa que SIGUE existiendo no se toca.
        if crate::custom_image::sanitize_config_assets(&mut config) {
            config.save();
        }
        let (hotkey, hotkey_rx) = match HotkeyManager::register(&config.shortcut()) {
            Ok(m) => {
                let rx = m.listen();
                (m, rx)
            }
            Err(e) => {
                crate::log!("hotkey", "{e}");
                let stub = HotkeyManager::stub(&config.shortcut());
                let rx = stub.listen();
                (stub, rx)
            }
        };
        let hotkey_label = hotkey.label();
        // Placeholder del editor (se calcula ANTES del struct literal: la
        // etiqueta se mueve dentro de él).
        let placeholder_text = transcript_placeholder(&hotkey_label);

        // Segunda ventana: la burbuja flotante. Se abre al arrancar; cuando la
        // ventana exista llegará `Message::BubbleOpened` y se le aplicará
        // WS_EX_NOACTIVATE + posición/esquina + tamaño (apply_bubble_extras).
        let (bubble_id, bubble_task) = {
            let (id, task) = window::open(crate::bubble::bubble_settings(&config));
            (Some(id), task.map(Message::BubbleOpened))
        };

        // Bandeja del sistema. Se crea aquí (hilo principal, el mismo donde
        // corre el bucle de winit): en Windows, tray-icon crea su ventana
        // oculta en el hilo llamante y los eventos llegan por canal global.
        // Si falla, la app sigue funcionando sin bandeja (se avisa en el log).
        let tray = match crate::tray::SystemTray::new() {
            Ok(tray) => Some(tray),
            Err(e) => {
                crate::log!("tray", "{e}");
                None
            }
        };

        // Fuente emoji (Segoe UI Emoji en Windows): que los símbolos de la UI
        // (✓ ☀ ☾ ▾ ▸ ● → ✕) no aparezcan como cajas vacías. La carga añade
        // los bytes al FontSystem de Iced; los textos se re-shapean solos al
        // cambiar la versión del FontSystem (ver src/fonts.rs).
        let font_task = match crate::fonts::load_emoji_font() {
            Some(task) => task.map(Message::FontLoaded),
            None => Task::none(),
        };

        let mut app = Self {
            config,
            status: Status::Idle,
            transcript: String::new(),
            last_turn: String::new(),
            removed_fillers: vec![],
            last_correction: None,
            session: CorrectionSession::new(),
            audio: None,
            input_devices: crate::audio::input_devices(),
            transcribe_rx: None,
            hotkey_rx,
            hotkey,
            hotkey_label,
            placeholder_text,
            transcript_editor: text_editor::Content::new(),
            copied_at: None,
            clear_armed: false,
            clear_armed_at: None,
            cleared_backup: None,
            dl_rate: None,
            dl_last: None,
            entry_from: String::new(),
            entry_to: String::new(),
            filler_input: String::new(),
            filler_category: FillerCategory::Unambiguous,
            settings_open: false,
            settings_section: SettingsSection::General,
            palette_inputs: std::collections::HashMap::new(),
            bubble_id,
            bubble_pressed: false,
            bubble_last_pos: Point::new(0.0, 0.0),
            config_save_deadline: None,
            bubble_right: crate::bubble::RightDragState::default(),
            bubble_drag: None,
            pending_inject: false,
            bubble_frame_index: 0,
            llm_rx: None,
            llm_pending: None,
            pending_diff: None,
            undo_state: None,
            tray,
            main_window_id: None,
            main_visible: true,
            // Arranque: el tema del sistema se lee una vez (el Poll lo
            // refresca cada 5 s si el modo es Auto) y la burbuja nace visible
            // (sin timer de auto-ocultar armado).
            system_dark: theme::system_dark_mode(),
            last_theme_check: std::time::Instant::now(),
            bubble_hide_deadline: None,
            bubble_hidden: false,
            // Arranque: la app levanta TODO por sí misma en background —
            // pantalla de carga visible (spinner + fase) y dictado bloqueado
            // hasta que whisper está listo (requisito UX 09/09/2026).
            startup: StartupPhase::Loading,
            llm_status: LlmStatus::Unknown,
            startup_rx: None,
            llm_phase_cancel: None,
            startup_label: "Preparando la app…".to_string(),
            startup_tick: 0,
            recording_started: None,
            device_lost: false,
            pending_turn_note: None,
        };

        // Hilo de arranque: descarga si hace falta + carga whisper en cache
        // + verifica/lanza el server LLM. Los eventos llegan por canal y el
        // Poll los procesa (mismo patrón que transcribe_rx/llm_rx). La
        // pantalla de carga se ve desde el primer frame.
        let (startup_tx, startup_rx) = mpsc::channel();
        let startup_config = app.config.clone();
        let startup_cancel = Arc::new(AtomicBool::new(false));
        let startup_cancel_thread = startup_cancel.clone();
        std::thread::spawn(move || {
            run_startup_thread(startup_config, startup_tx, startup_cancel_thread)
        });
        app.startup_rx = Some(startup_rx);
        app.llm_phase_cancel = Some(startup_cancel);

        (app, Task::batch([bubble_task, font_task]))
    }

    pub(crate) fn save_config(&mut self) {
        self.config_save_deadline = Some(std::time::Instant::now() + Duration::from_millis(300));
    }

    /// Toma el buffer en vuelo de `palette_inputs[field]` (si existe) o el
    /// hex ya persistido (si el usuario solo tocó «Aplicar» sin teclear);
    /// parsea robusto; persiste; vacía el buffer. Si el input está vacío se
    /// interpreta como «restaurar este campo al default» (= `None`).
    pub(crate) fn apply_palette_field(&mut self, field: theme::PaletteField) {
        let raw_input = self
            .palette_inputs
            .remove(&field)
            .unwrap_or_else(|| {
                // Sin buffer → tomar el valor ya persistido para mantenerlo.
                let cfg = self.config.palette.as_ref();
                let current = match cfg {
                    Some(c) => match field {
                        theme::PaletteField::Bg => c.bg.clone(),
                        theme::PaletteField::Surface => c.surface.clone(),
                        theme::PaletteField::SurfaceAlt => c.surface_alt.clone(),
                        theme::PaletteField::Text => c.text.clone(),
                        theme::PaletteField::Dim => c.dim.clone(),
                        theme::PaletteField::Accent => c.accent.clone(),
                        theme::PaletteField::AccentHover => c.accent_hover.clone(),
                        theme::PaletteField::AccentText => c.accent_text.clone(),
                        theme::PaletteField::Ok => c.ok.clone(),
                        theme::PaletteField::Warn => c.warn.clone(),
                        theme::PaletteField::Err => c.err.clone(),
                        theme::PaletteField::Border => c.border.clone(),
                    },
                    None => None,
                };
                current.unwrap_or_default()
            });
        // Trim antes de decidir vacío.
        let trimmed = raw_input.trim().to_string();
        if trimmed.is_empty() {
            // Vacío = restaurar default (None en el slot).
            let cfg = self
                .config
                .palette
                .get_or_insert_with(theme::PaletteConfig::default);
            cfg.clear_field(field);
            if cfg.is_empty() {
                self.config.palette = None;
            }
        } else if theme::parse_hex(&trimmed).is_some() {
            let cfg = self
                .config
                .palette
                .get_or_insert_with(theme::PaletteConfig::default);
            match field {
                theme::PaletteField::Bg => cfg.bg = Some(trimmed),
                theme::PaletteField::Surface => cfg.surface = Some(trimmed),
                theme::PaletteField::SurfaceAlt => cfg.surface_alt = Some(trimmed),
                theme::PaletteField::Text => cfg.text = Some(trimmed),
                theme::PaletteField::Dim => cfg.dim = Some(trimmed),
                theme::PaletteField::Accent => cfg.accent = Some(trimmed),
                theme::PaletteField::AccentHover => cfg.accent_hover = Some(trimmed),
                theme::PaletteField::AccentText => cfg.accent_text = Some(trimmed),
                theme::PaletteField::Ok => cfg.ok = Some(trimmed),
                theme::PaletteField::Warn => cfg.warn = Some(trimmed),
                theme::PaletteField::Err => cfg.err = Some(trimmed),
                theme::PaletteField::Border => cfg.border = Some(trimmed),
            }
            // Tras persistir, vacía el buffer para que el input «libere»
            // el hex en vuelo (la UI mostrará el valor persistido).
            self.palette_inputs.remove(&field);
        } else {
            // Hex inválido: se IGNORA (requisito: NUNCA romper la UI). Log
            // para diagnóstico y vacía el buffer para que el input NO siga
            // mostrando basura (la regla «input vacío = default» aplica).
            crate::log!(
                "palette",
                "hex inválido descartado en {:?}: «{}»",
                field,
                trimmed
            );
            self.palette_inputs.remove(&field);
        }
        self.save_config();
    }

    // ── Fusión de paletas (import) ─────────────────────────────────────────
    // Un campo presente en `incoming` sobrescribe el de `dest`; un campo
    // ausente mantiene el valor actual. Esto permite compartir paletas
    // parciales (p. ej. solo el acento). Hex inválido en `incoming` se
    // ignora silenciosamente para no romper la paleta actual.
    fn merge_palette_field(
        dest: &mut Option<String>,
        incoming: &Option<String>,
    ) {
        if let Some(raw) = incoming.as_deref() {
            if theme::parse_hex(raw).is_some() {
                *dest = Some(raw.to_string());
            }
        }
    }

    pub(crate) fn merge_palette_configs(
        dest: &mut theme::PaletteConfig,
        incoming: &theme::PaletteConfig,
    ) {
        Self::merge_palette_field(&mut dest.bg, &incoming.bg);
        Self::merge_palette_field(&mut dest.surface, &incoming.surface);
        Self::merge_palette_field(&mut dest.surface_alt, &incoming.surface_alt);
        Self::merge_palette_field(&mut dest.text, &incoming.text);
        Self::merge_palette_field(&mut dest.dim, &incoming.dim);
        Self::merge_palette_field(&mut dest.accent, &incoming.accent);
        Self::merge_palette_field(&mut dest.accent_hover, &incoming.accent_hover);
        Self::merge_palette_field(&mut dest.accent_text, &incoming.accent_text);
        Self::merge_palette_field(&mut dest.ok, &incoming.ok);
        Self::merge_palette_field(&mut dest.warn, &incoming.warn);
        Self::merge_palette_field(&mut dest.err, &incoming.err);
        Self::merge_palette_field(&mut dest.border, &incoming.border);
    }

    pub(crate) fn persist_config(&self) {
        let mut cfg = self.config.clone();
        cfg.dictionary = self.dictionary_snapshot();
        cfg.model = Some(self.config.selected_model().id().to_string());
        cfg.save();
    }

    pub(crate) fn flush_config(&mut self) {
        let Some(deadline) = self.config_save_deadline else {
            return;
        };
        if std::time::Instant::now() < deadline {
            return;
        }
        self.persist_config();
        self.config_save_deadline = None;
    }

    /// Inyección con la configuración actual (formato, retardo y filtro por
    /// ventana de `config.json`). Feedback honesto en los tres casos: éxito →
    /// mensaje verde; filtro que no coincide → aviso (no es error); fallo
    /// técnico → `Status::Error`.
    pub(crate) fn inject_into_active(&mut self, text: &str) {
        match inject_text_into_active_app(text, &self.config.inject_settings()) {
            Ok(InjectOutcome::Injected) => {
                self.last_correction = Some(InjectOutcome::Injected.notice());
            }
            Ok(InjectOutcome::Skipped(_)) => {
                self.last_correction = Some(InjectOutcome::Skipped(String::new()).notice());
            }
            Ok(InjectOutcome::NoForegroundWindow) => {
                // Sin ninguna ventana con el foco no hay dónde pegar: el
                // turno no se pierde — queda en el portapapeles para que el
                // usuario lo pegue donde quiera (requisito de robustez).
                if let Err(e) = crate::inject::copy_to_clipboard(text) {
                    crate::log!("inject", "sin ventana activa y sin portapapeles: {e}");
                }
                self.last_correction = Some(InjectOutcome::NoForegroundWindow.notice());
            }
            Err(e) => self.status = Status::Error(format!("Inyección: {e}")),
        }
    }

    /// Sincroniza el editor multilínea con `self.transcript` tras un cambio
    /// PROGRAMÁTICO del texto (turno nuevo, corrección, undo, borrado): se
    /// reconstruye el contenido y el cursor va al final (el turno nuevo se
    /// ve al instante, como el `anchor_bottom` del antiguo scrollable).
    /// Las ediciones MANUALES no pasan por aquí (van por
    /// `Message::TranscriptEdited`): así editar a mano y seguir dictando
    /// no se pisan — el dictado añade al final del texto YA editado.
    pub(crate) fn sync_editor_from_transcript(&mut self) {
        self.transcript_editor = text_editor::Content::with_text(&self.transcript);
        // Cursor al final del documento: mantiene lo nuevo a la vista.
        self.transcript_editor
            .perform(text_editor::Action::Move(text_editor::Motion::DocumentEnd));
    }

    /// Registra una muestra de progreso de descarga: actualiza el estado
    /// (bytes descargados/total) y la velocidad estimada (EMA al 30 %) que
    /// alimenta el «quedan ~X» del hero y de la pantalla de carga. Una
    /// descarga NUEVA (bytes que retroceden) reinicia el tracker.
    pub(crate) fn note_download_progress(&mut self, downloaded: u64, total: u64) {
        if let Status::Downloading { model, .. } = &self.status {
            self.status = Status::Downloading {
                model: *model,
                downloaded,
                total,
            };
        }
        let now = std::time::Instant::now();
        match self.dl_last {
            // Muestra válida solo si avanza y ha pasado medio segundo (las
            // ráfagas de mensajes instantáneos miden velocidad infinita).
            Some((prev_d, prev_t)) if downloaded >= prev_d => {
                let dt = now.duration_since(prev_t).as_secs_f64();
                if dt >= 0.5 {
                    let inst = (downloaded - prev_d) as f64 / dt;
                    self.dl_rate = Some(match self.dl_rate {
                        Some(prev) => prev * 0.7 + inst * 0.3,
                        None => inst,
                    });
                    self.dl_last = Some((downloaded, now));
                }
            }
            // Primera muestra o descarga nueva: aún sin velocidad fiable.
            _ => {
                self.dl_last = Some((downloaded, now));
                self.dl_rate = None;
            }
        }
    }

    /// Núcleo de fin de turno, compartido por la parada voluntaria
    /// (`StopRecording`: atajo, botón, burbuja o tope de duración) y la
    /// forzosa (`AudioDeviceLost`: micro desconectado a mitad). Recoge el
    /// audio acumulado y lo manda al hilo de transcripción; si no se oyó
    /// nada, el turno termina limpio con un aviso visible (nunca un
    /// silencio confuso ni un estado colgado).
    pub(crate) fn finish_recording(&mut self) -> Task<Message> {        let Some(mut capture) = self.audio.take() else {
            return Task::none();
        };
        self.recording_started = None;
        let samples = capture.stop();
        if samples.is_empty() {
            self.pending_inject = false;
            self.status = Status::Idle;
            // Sin audio no hay transcripción que esperar: el aviso se pone
            // aquí y ya (0 s de grabación, doble pulsación del atajo, o
            // micro muerto antes de oír nada).
            self.last_correction = Some(if self.device_lost {
                self.device_lost = false;
                "El micrófono se desconectó antes de que se oyera nada; revisa el micro y vuelve a dictar.".to_string()
            } else {
                "No se oyó nada: comprueba el micrófono, acércate y repite el dictado.".to_string()
            });
            return Task::none();
        }

        let model = self.config.selected_model();
        // El diccionario viaja como initial_prompt: sesga el
        // reconocedor hacia jerga/nombres/siglas ANTES de transcribir.
        let dictionary = self.config.dictionary.clone();
        // Parámetros de inferencia ajustados en Ajustes. Se clonan
        // aquí para mover al hilo de transcripción sin necesidad de
        // un Arc.
        let inference = crate::transcribe::InferenceParams {
            language: self.config.language().map(str::to_string),
            beam_size: self.config.beam_size() as i32,
            n_threads: self.config.n_threads() as i32,
        };
        let (tx, rx) = mpsc::channel();
        self.transcribe_rx = Some(rx);
        self.status = Status::Transcribing;

        std::thread::spawn(move || {
            let result = transcribe_samples(samples, model, dictionary, inference);
            let _ = tx.send(result);
        });

        Task::none()
    }

    /// Despachador unificado del resultado LLM (t_78d9e1fa). Llega por
    /// `Message::CorrectionDone` o `Message::RedactionDone` (ambos
    /// llevan `Result<LlmResult, String>`); este método decide el modo
    /// mirando `llm_pending.mode` y enruta a `handle_llm_diff` (cross)
    /// o `handle_llm_redaction` (in-turn). En caso de error del LLM,
    /// ambos modos acaban en `restore_pending_as_dictation` para no
    /// perder el texto del usuario.
    pub(crate) fn handle_llm_correction(
        &mut self,
        result: Result<LlmResult, String>,
    ) -> Task<Message> {
        let pending = self.llm_pending.take();
        let mode = pending.as_ref().map(|p| match p.mode {
            LlmMode::CrossTurn => "cross-turn",
            LlmMode::InTurn => "in-turn",
        }).unwrap_or("none");
        match result {
            Ok(LlmResult::Diff(diff)) => self.handle_llm_diff(diff, pending),
            Ok(LlmResult::Redacted(redacted)) => self.handle_llm_redaction(redacted, pending),
            Err(e) => {
                self.last_correction = Some(self.format_llm_error(&e));
                crate::log!(
                    "llm",
                    "resultado del corrector {}: error (recuperado como dictado normal): {}",
                    mode,
                    e
                );
                self.restore_pending_as_dictation(pending);
                self.status = Status::Idle;
                self.arm_bubble_hide_timeout();
                self.save_config();
                Task::none()
            }
        }
    }

    /// Formatea un error del LLM con un hint orientado al usuario
    /// (mismo contrato que la versión previa de handle_llm_correction).
    fn format_llm_error(&self, e: &str) -> String {
        let hint = if e.starts_with("HTTP:") {
            " (¿está el servidor LLM arrancado? Mira el log en %APPDATA%\\whispr-open)"
        } else if e.starts_with("el proveedor cloud requiere una API key") {
            " (configura la API key del proveedor en Ajustes)"
        } else {
            ""
        };
        format!("Corrección IA no disponible: {e}{hint}")
    }

    /// Modo cross-turn: aplica el diff a `target` (el último turno).
    /// Migrado desde el antiguo `handle_llm_correction` sin cambios de
    /// comportamiento; solo se aísla para que el flujo in-turn no le
    /// afecte.
    fn handle_llm_diff(
        &mut self,
        diff: crate::domain::corrective::Diff,
        pending: Option<LlmPending>,
    ) -> Task<Message> {
        self.llm_rx = None;

        let Some(p) = pending else {
            self.status = Status::Idle;
            return Task::none();
        };
        let target = p.target.clone();

        // LOG: el LLM devolvió algo (válido o vacío). Antes esto
        // era invisible; ahora se ve siempre.
        if diff.find.is_empty() {
            crate::log!(
                "llm",
                "corrector (cross-turn) devolvió diff vacío: instruction={:?} target={:?} (se recupera como dictado normal)",
                p.instruction,
                target
            );
        } else {
            crate::log!(
                "llm",
                "corrector (cross-turn) devolvió diff: find={:?} replace={:?} target={:?}",
                diff.find,
                diff.replace,
                target
            );
        }

        // Modo confirmar (spec §4.2): si el toggle está activo,
        // no aplicamos a ciegas. Guardamos el diff en `pending_diff`
        // y dejamos que la UI muestre «Aplicar» / «Descartar».
        if self.config.ai_correction_confirm() {
            self.pending_diff = Some(PendingDiff {
                diff: diff.clone(),
                target,
                force_inject: p.force_inject,
            });
            self.last_correction = Some(format!(
                "Rectificado IA propuesto: «{}» → «{}» (Aplicar/Descartar)",
                diff.find, diff.replace
            ));
            self.status = Status::Idle;
            return Task::none();
        }

        // Modo fluido (default): aplicar inmediatamente.
        match crate::domain::corrective::apply_diff(&target, &diff) {
            Some(after) => {
                // Undo de 1 nivel ANTES de mutar (estado completo).
                self.undo_state = Some((self.transcript.clone(), self.last_turn.clone()));
                if let Some(t) = crate::domain::corrective::replace_last_turn(
                    &self.transcript,
                    &target,
                    &after,
                ) {
                    self.transcript = t;
                    self.sync_editor_from_transcript();
                } else {
                    self.last_correction =
                        Some("La corrección IA ya no aplica al último turno".to_string());
                    self.restore_pending_as_dictation(Some(p));
                    self.status = Status::Idle;
                    self.arm_bubble_hide_timeout();
                    return Task::none();
                }
                if self.last_turn == target {
                    self.last_turn = after.clone();
                }
                self.last_correction = Some(format!(
                    "Rectificado IA: “{}” → “{}”",
                    diff.find, diff.replace
                ));
                crate::log!("llm", "rectificado (cross-turn): {} → {}", diff.find, diff.replace);

                let force_inject = p.force_inject;
                if (self.config.auto_inject.unwrap_or(true) || force_inject)
                    && !after.is_empty()
                {
                    self.inject_into_active(&after);
                }
            }
            None => {
                self.last_correction =
                    Some("De momento no sé qué corregir en el último turno".to_string());
                self.restore_pending_as_dictation(Some(p));
            }
        }
        self.status = Status::Idle;
        self.arm_bubble_hide_timeout();
        self.save_config();
        Task::none()
    }

    /// Modo in-turn (t_78d9e1fa, spec §3.5): el LLM devuelve el texto
    /// final corregido (sin meta-lengua). Lo aplicamos COMO el turno
    /// completo (no como diff sobre `last_turn`):
    ///
    ///   - `self.transcript` se extiende con el texto corregido.
    ///   - `self.last_turn` pasa a ser el texto corregido.
    ///   - Se inyecta el texto corregido en la app activa (mismo
    ///     contrato que un dictado normal exitoso).
    ///   - El undo de 1 nivel guarda el estado PREVIO (transcript +
    ///     last_turn) por si el usuario quiere revertir.
    ///
    /// Casos de fallback al input bruto:
    ///   - El LLM devuelve vacío (modelo roto, respuesta vacía).
    ///   - El LLM no eliminó la meta-lengua (regex de in-turn todavía
    ///     presente en el output → la corrección falló).
    ///   - El LLM devuelve el mismo texto que el input (puede ser
    ///     legítimo: «no hay corrección»; en ese caso, treat como
    ///     input ya limpio y sigue el flujo normal).
    fn handle_llm_redaction(
        &mut self,
        redacted: String,
        pending: Option<LlmPending>,
    ) -> Task<Message> {
        self.llm_rx = None;

        let Some(p) = pending else {
            self.status = Status::Idle;
            return Task::none();
        };
        let input = p.target.clone(); // el turno completo enviado al LLM
        let force_inject = p.force_inject;

        crate::log!(
            "llm",
            "corrector (in-turn) devolvió: input={:?} output={:?}",
            input,
            redacted
        );

        // Filtros de seguridad: si el output está vacío o contiene
        // todavía un marcador in-turn, NO usamos la respuesta:
        // fallback al input bruto (spec §3.5.4).
        let clean_text = redacted.trim();
        if clean_text.is_empty() {
            crate::log!(
                "llm",
                "in-turn: respuesta vacía del LLM, fallback al input bruto"
            );
            self.last_correction = Some(
                "Corrección IA no pudo reescribir el turno (texto vacío)".to_string(),
            );
            self.inject_or_store_raw(&input, force_inject);
            self.status = Status::Idle;
            self.arm_bubble_hide_timeout();
            self.save_config();
            return Task::none();
        }
        // Guard numérico determinista (t_1fc864ef, ampliado t_d64797c2). El
        // corrector local (Qwen3.5-0.8B-Q8) concatena numerales en lugar de
        // sustituirlos cuando el usuario rectifica un número: «me comí
        // cuatro digo cinco peras» → «me comí cuatro cinco peras» (sin
        // «digo», con los dos números juntos); y en frases naturales se
        // queda con el número VIEJO y borra la meta-lengua («a tres
        // personas, digo cuatro personas» → «a tres personas») — frase
        // plausible y FALSA (log 14/09 03:19 y 13/09 01:37). Este guard
        // detecta el patrón «NUM_A [texto] [marcador] NUM_B» en el INPUT y
        // reconstruye determinísticamente la sustitución. Si el LLM ya
        // acertó (contiene NUM_B pero no NUM_A), el guard NO interviene y
        // deja pasar el output tal cual.
        //
        // El texto del guard es el que se inyecta (`effective_clean_text`,
        // usado más abajo): antes se calculaba pero se descartaba, así que
        // la reconstrucción nunca llegaba al usuario.
        let effective_clean_text = match crate::llm::guard_numeric_substitution(&input, clean_text)
        {
            Ok(reconstructed) => {
                crate::log!(
                    "llm",
                    "in-turn: guard numérico aplicado: {:?} -> {:?}",
                    clean_text,
                    reconstructed
                );
                reconstructed
            }
            Err(()) => clean_text.to_string(),
        };
        let still_has_meta =
            crate::domain::corrective::contains_in_turn_marker(&effective_clean_text);
        // Guard anti-truncación (t_6b9348de): el LLM local (Qwen3.5-0.8B)
        // responde a los turnos largos sin corrección con UN RESUMEN
        // («estamos debulando la aplicación…» → «no se ha levantado ningún
        // servidor», log 01:34:54). El prompt no se endurece porque toda
        // variante «no resumas» rompe alguna auto-corrección (verificado
        // en runtime); este guard es la capa OBLIGATORIA: si la respuesta
        // queda desproporcionadamente corta respecto al input sin
        // auto-corrección inequívoca que lo justifique, se descarta y se
        // inyecta el input ORIGINAL — nunca se pierde ni se recorta el
        // dictado (ver `guard_anti_truncacion`). Se evalúa sobre el texto
        // que de verdad se va a inyectar (reconstrucción incluida): los
        // umbrales y la lógica del guard no cambian.
        if crate::llm::guard_anti_truncacion(&input, &effective_clean_text) != effective_clean_text {
            crate::log!(
                "llm",
                "in-turn: respuesta recortada respecto al input (guard anti-truncación), fallback al input bruto: input={:?} output={:?}",
                input,
                effective_clean_text
            );
            self.last_correction = Some(
                "La corrección IA devolvió un resumen del turno; se mantiene el texto original"
                    .to_string(),
            );
            self.inject_or_store_raw(&input, force_inject);
            self.status = Status::Idle;
            self.arm_bubble_hide_timeout();
            self.save_config();
            return Task::none();
        }
        if still_has_meta {
            crate::log!(
                "llm",
                "in-turn: la respuesta todavía contiene meta-lengua correctiva ({:?}), fallback al input bruto",
                effective_clean_text
            );
            self.last_correction = Some(
                "Corrección IA no pudo limpiar el turno (sigue con meta-lengua)".to_string(),
            );
            self.inject_or_store_raw(&input, force_inject);
            self.status = Status::Idle;
            self.arm_bubble_hide_timeout();
            self.save_config();
            return Task::none();
        }

        // Caso normal: tenemos texto limpio. Lo aplicamos como el
        // turno completo. Undo antes de mutar. El texto aplicado es el
        // que resolvieron los guards (`effective_clean_text`): la
        // reconstrucción numérica determinista, si la hubo, ES el texto
        // final (t_1fc864ef + t_d64797c2).
        self.undo_state = Some((self.transcript.clone(), self.last_turn.clone()));
        let clean = effective_clean_text.clone();
        self.transcript = crate::domain::correction::append_transcript(
            &self.transcript,
            &clean,
        );
        self.sync_editor_from_transcript();
        self.last_turn = clean.clone();
        let removed = self.removed_fillers_summary();
        let summary = if input == clean {
            format!("Turno confirmado por IA (sin cambios): «{}»", clean)
        } else {
            // Mostramos la corrección aplicada: «antes → después» con
            // el meta-lengua destacada.
            let stripped_meta = crate::domain::corrective::strip_in_turn_meta(&input);
            format!(
                "Auto-corregido IA: «{}» → «{}»",
                stripped_meta.trim(),
                clean
            )
        };
        self.last_correction = Some(summary);
        crate::log!(
            "llm",
            "in-turn aplicado: «{}» → «{}»",
            input,
            clean
        );

        if (self.config.auto_inject.unwrap_or(true) || force_inject) && !clean.is_empty() {
            self.inject_into_active(&clean);
        }
        // Silenciar warning de variable no usada (removed fillers: hay
        // feedback útil si el filtro actuó sobre el turno limpio, pero
        // por ahora solo se loguea para diagnóstico).
        let _ = removed;
        self.status = Status::Idle;
        self.arm_bubble_hide_timeout();
        self.save_config();
        Task::none()
    }

    /// Fallback unificado para cuando el LLM falla o devuelve algo no
    /// utilizable (cross-turn o in-turn). El turno original se inyecta
    /// tal cual o se trata como dictado normal (spec §3.3.1 y §3.5.4:
    /// nunca perder texto del usuario).
    pub(crate) fn restore_pending_as_dictation(&mut self, pending: Option<LlmPending>) {
        let Some(p) = pending else { return };
        let text = if p.target.is_empty() {
            return;
        } else if matches!(p.mode, LlmMode::InTurn) {
            p.target.clone()
        } else {
            // Cross-turn: la instrucción es lo que el usuario QUIERE
            // que aparezca si la corrección falla.
            if !p.instruction.is_empty() {
                p.instruction.clone()
            } else {
                p.target.clone()
            }
        };
        self.inject_or_store_raw(&text, p.force_inject);
    }

    /// Inyecta `text` si el toggle está activo (o si `force_inject`),
    /// y si no lo inyecta, lo acumula al menos como `last_turn` para
    /// que el siguiente turno pueda corregirlo.
    fn inject_or_store_raw(&mut self, text: &str, force_inject: bool) {
        if text.is_empty() {
            return;
        }
        let should_update_turn = self.last_turn.is_empty();
        self.transcript =
            crate::domain::correction::append_transcript(&self.transcript, text);
        if should_update_turn {
            self.last_turn = text.to_string();
        }
        if self.config.auto_inject.unwrap_or(true) || force_inject {
            self.inject_into_active(text);
        }
    }

    /// Resumen de las muletillas eliminadas en el turno actual (si
    /// las hay). Solo se usa para diagnóstico en logs; reservado
    /// para una versión futura del feedback de UI.
    fn removed_fillers_summary(&self) -> String {
        self.removed_fillers.join(", ")
    }

    /// Aplica el diff pendiente (modo `ai_correction_confirm`). Reconstruye
    /// la «aplicación a ciegas» pero sobre el diff que el usuario ya vio:
    /// mismos pasos que el camino Ok de `handle_llm_correction` (capturar
    /// undo, sustituir, inyectar si corresponde). Si `pending_diff` es
    /// None (carrera: el usuario toggleo off mientras decidía), no-op.
    pub(crate) fn apply_pending_diff(&mut self) {
        let Some(pending) = self.pending_diff.take() else {
            return;
        };
        // Aplicar siempre sobre el target capturado, nunca sobre un turno
        // posterior que el usuario haya dictado mientras decidía.
        let target = pending.target;
        let diff = pending.diff;
        match crate::domain::corrective::apply_diff(&target, &diff) {
            Some(after) => {
                self.undo_state = Some((self.transcript.clone(), self.last_turn.clone()));
                if let Some(t) =
                    crate::domain::corrective::replace_last_turn(&self.transcript, &target, &after)
                {
                    self.transcript = t;
                    self.sync_editor_from_transcript();
                } else {
                    self.last_correction =
                        Some("La corrección IA ya no aplica al último turno".to_string());
                    return;
                }
                if self.last_turn == target {
                    self.last_turn = after.clone();
                }
                self.last_correction = Some(format!(
                    "Rectificado IA: “{}” → “{}”",
                    diff.find, diff.replace
                ));
                crate::log!(
                    "llm",
                    "rectificado (confirmado): {} → {}",
                    diff.find,
                    diff.replace
                );
                if (self.config.auto_inject.unwrap_or(true) || pending.force_inject)
                    && !after.is_empty()
                {
                    self.inject_into_active(&after);
                }
            }
            None => {
                // El diff ya no aplica al target capturado: no mutamos el
                // transcript ni inyectamos nada. La instrucción solo se
                // recupera como dictado normal cuando el LLM falla, no cuando
                // el usuario descarta explícitamente una propuesta.
                self.last_correction =
                    Some("La corrección IA ya no aplica al último turno".to_string());
            }
        }
    }

    /// Descarta el diff pendiente. El usuario ya vio la propuesta y la
    /// rechaza explícitamente: no se modifica el transcript ni se inyecta la
    /// instrucción de corrección como texto normal.
    pub(crate) fn discard_pending_diff(&mut self) {
        let Some(_pending) = self.pending_diff.take() else {
            return;
        };
        self.last_correction = Some("Corrección IA descartada ✕".to_string());
    }

    // ── Accesores para la burbuja flotante (bubble.rs) ─────────────────────
    pub(crate) fn is_bubble_window(&self, id: window::Id) -> bool {
        Some(id) == self.bubble_id
    }

    pub(crate) fn bubble_status(&self) -> &Status {
        &self.status
    }

    /// ¿El arranque aún está en curso (whisper/LLM cargándose)? Mientras sea
    /// cierto, el dictado está bloqueado y la vista principal es la pantalla
    /// de carga. Lo usa la burbuja para mostrar el ámbar de «cargando».
    pub(crate) fn is_starting(&self) -> bool {
        matches!(self.startup, StartupPhase::Loading)
    }

    /// Color de la burbuja durante el arranque: ámbar mientras carga, rojo
    /// si el arranque falló sin modelo transcribible. None = el arranque no
    /// pinta la burbuja (deja que `bubble_status` decida).
    pub(crate) fn startup_bubble_color(&self) -> Option<Color> {
        match &self.startup {
            StartupPhase::Loading => Some(crate::theme::WARN),
            StartupPhase::Failed { .. } => Some(crate::theme::ERR),
            StartupPhase::Ready => None,
        }
    }

    /// Actualiza el ítem de estado del menú de bandeja (best-effort: sin
    /// bandeja no hay nada que actualizar).
    fn set_tray_status(&self, label: &str) {
        if let Some(tray) = &self.tray {
            tray.status_item.set_text(label);
        }
    }

    /// Relanza la fase LLM en background (toggle de corrección IA activado
    /// en caliente tras un arranque sin server). Eventos por `startup_rx`;
    /// el Poll los procesa igual que los del arranque inicial.
    ///
    /// Cada fase recibe un token de cancelación nuevo (`llm_phase_cancel`):
    /// si el usuario la apaga mientras la fase está en vuelo, el token se
    /// pone a true y el hilo aborta en su siguiente punto de control (sin
    /// lanzar server, o terminando el PID que él mismo lanzó).
    pub(crate) fn spawn_llm_startup(&mut self) -> Task<Message> {
        let (startup_tx, startup_rx) = mpsc::channel();
        let startup_config = self.config.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_thread = cancel.clone();
        std::thread::spawn(move || run_llm_phase(startup_config, startup_tx, cancel_thread));
        self.startup_rx = Some(startup_rx);
        self.llm_phase_cancel = Some(cancel);
        Task::none()
    }

    /// Apaga la corrección IA en caliente y deja la máquina de estados
    /// coherente (ciclo de vida 10/09/2026, requisito «descargar el modelo
    /// al desactivar»):
    ///
    /// 1. Corrección LLM pendiente → degrada a texto bruto (spec: nunca
    ///    perder texto; el turno se inyecta tal cual, sin esperar al LLM).
    /// 2. Propuesta de diff en confirmación pendiente → se descarta (el
    ///    texto bruto del target ya está en el transcript).
    /// 3. Fase LLM en vuelo → cancelada: no debe lanzar un server después
    ///    de haberse apagado (el token la aborta en su punto de control y
    ///    termina su propio PID si ya lo lanzó).
    /// 4. Server lanzado por ESTA app → terminado (modelo descargado de
    ///    RAM/VRAM). Un server ajeno (ya corría antes de abrir la app) NO
    ///    se toca: `kill_launched_server` solo mata el PID que registró
    ///    `launch_server`.
    pub(crate) fn disable_ai_correction_now(&mut self) {
        // 1) Corrección en vuelo → texto bruto.
        if self.llm_rx.is_some() || self.llm_pending.is_some() {
            if let Some(p) = self.llm_pending.take() {
                crate::log!(
                    "llm",
                    "corrección IA apagada con corrección en vuelo: degrada a texto bruto"
                );
                self.restore_pending_as_dictation(Some(p));
            }
            self.llm_rx = None;
        }
        // 2) Propuesta de diff pendiente → descartada (el texto bruto queda).
        if self.pending_diff.take().is_some() {
            crate::log!(
                "llm",
                "corrección IA apagada con propuesta pendiente: descartada, texto bruto intacto"
            );
        }
        if self.status == Status::Correcting {
            self.status = Status::Idle;
        }
        // 3) Cancelar la fase LLM en vuelo.
        if let Some(cancel) = &self.llm_phase_cancel {
            cancel.store(true, Ordering::SeqCst);
        }
        // 4) Descargar el modelo: terminar solo el server que lanzó la app.
        let killed = crate::llm::kill_launched_server();
        crate::log!(
            "llm",
            "corrección IA desactivada por el usuario (server propio terminado: {killed})"
        );
        self.llm_status = LlmStatus::Disabled(String::new());
        self.last_correction = Some(if killed {
            "Corrección IA desactivada — modelo LLM descargado de memoria ✓".to_string()
        } else {
            "Corrección IA desactivada".to_string()
        });
    }

    pub(crate) fn config_is_dark(&self) -> bool {
        // Modo resuelto: Auto → estado cacheado del sistema (AppsUseLightTheme,
        // refrescado en el Poll cada 5 s); Claro/Oscuro → forzado por config.
        self.config.resolved_dark(self.system_dark)
    }

    /// Tamaño del círculo de la burbuja (px lógicos) — accesor para bubble.rs
    /// (el campo `config` es privado de este módulo).
    pub(crate) fn bubble_size_px(&self) -> f32 {
        self.config.bubble_size().px()
    }

    /// Opacidad de la burbuja en % (20..=100) — accesor para bubble.rs.
    pub(crate) fn bubble_opacity_percent(&self) -> u8 {
        self.config.bubble_opacity()
    }

    pub(crate) fn bubble_style(&self) -> crate::config::BubbleStyle {
        self.config.bubble_style()
    }

    /// Frames resueltos para la animación por conmutación de la
    /// burbuja. `None` si la carpeta no está configurada, no existe o
    /// no contiene imágenes soportadas (en cuyo caso el render usa el
    /// sprite estático si está, o el render vectorial). Cachear el
    /// resultado de `resolve_frames` aquí evita I/O repetido durante
    /// el redibujo (que ocurre ~60 Hz).
    pub(crate) fn bubble_frames_resolved(
        &self,
    ) -> Option<Vec<std::path::PathBuf>> {
        // `*_or_warn` (H4): si la carpeta configurada desapareció, queda dicho
        // en el log («asset desaparecido, cayendo al tema») una sola vez.
        crate::custom_image::resolve_frames_or_warn(
            "carpeta de frames de la burbuja",
            self.config.bubble_frames_path(),
        )
    }

    /// Frame actual del sprite animado (la ruta del fichero a mostrar).
    /// Si la animación está parada o el índice quedó fuera de rango,
    /// devuelve el primer frame (determinista). Es el método que usa
    /// `bubble.rs` para pintar el sprite sin tocar el `Message` ni el
    /// `update`.
    pub(crate) fn bubble_current_frame(&self) -> Option<std::path::PathBuf> {
        let frames = self.bubble_frames_resolved()?;
        if frames.is_empty() {
            return None;
        }
        let idx = self
            .bubble_frame_index
            .min(frames.len().saturating_sub(1));
        Some(frames[idx].clone())
    }

    /// Ruta del sprite estático de la burbuja (validada). `None` si no
    /// hay imagen propia (en ese caso se pinta el render vectorial).
    pub(crate) fn bubble_static_image(
        &self,
    ) -> Option<std::path::PathBuf> {
        crate::custom_image::resolve_static_or_warn(
            "sprite de la burbuja",
            self.config.bubble_image_path(),
        )
    }

    /// Ruta de la imagen de fondo de la ventana principal (validada).
    /// `None` = fondo del tema.
    pub(crate) fn bg_image(&self) -> Option<std::path::PathBuf> {
        crate::custom_image::resolve_static_or_warn(
            "imagen de fondo de la ventana",
            self.config.bg_image_path(),
        )
    }

    fn dictionary_snapshot(&self) -> Vec<DictionaryEntry> {
        self.config.dictionary.clone()
    }

    /// Nivel del micrófono en % (0-100) mientras se graba.
    /// Sección visible de la ventana principal (UI 24/09/2026).
    ///
    /// `settings_open` sigue siendo el ÚNICO estado (lo tocan los handlers de
    /// Ajustes y su persistencia); aquí solo se LEE para saber qué pestaña
    /// está activa. Así la vista gana pestañas sin duplicar estado.
    pub(crate) fn window_section(&self) -> WindowSection {
        if self.settings_open {
            WindowSection::Ajustes
        } else {
            WindowSection::Dictado
        }
    }

    /// Segundos que lleva el turno en curso (0 si no se está grabando). Lo
    /// usa el aviso de «sin señal» del medidor para no parpadear en el
    /// primer instante del turno.
    fn recording_secs(&self) -> u64 {
        self.recording_started
            .map(|inicio| inicio.elapsed().as_secs())
            .unwrap_or(0)
    }

    /// Aviso de nivel del turno en curso (`None` si el nivel es normal).
    fn mic_peak_hint(&self) -> Option<MicPeakHint> {
        mic_peak_hint(self.mic_level_percent(), self.recording_secs())
    }

    fn mic_level_percent(&self) -> u32 {
        self.audio
            .as_ref()
            .map(|a| (a.level() * 100.0) as u32)
            .unwrap_or(0)
    }

    /// Lectura del estado para el hero: color de estado + label + hint
    /// (feedback doble, nunca solo color).
    fn mic_ui(&self, t: &UiPalette) -> MicUi {
        match &self.status {
            Status::Idle => MicUi {
                color: t.dim,
                hint_color: t.dim,
                label: "Listo".to_string(),
                hint: format!(
                    "Pulsa el círculo o {} y empieza a hablar",
                    self.hotkey_label
                ),
            },
            Status::Recording => MicUi {
                // accent_text (no accent): «Grabando…» es texto normal
                // (fs.md) sobre tarjeta; el violeta de marca mide ~4.2:1
                // ahí (ver tests de contraste en theme.rs).
                color: t.accent_text,
                hint_color: t.dim,
                label: "Grabando…".to_string(),
                hint: "Suelta el atajo o pulsa para transcribir".to_string(),
            },
            Status::Transcribing => MicUi {
                color: t.warn,
                hint_color: t.dim,
                label: "Transcribiendo…".to_string(),
                hint: "Procesando el audio con Whisper".to_string(),
            },
            Status::Correcting => MicUi {
                color: t.warn,
                hint_color: t.dim,
                label: "Corrigiendo con IA…".to_string(),
                hint: "La IA de corrección revisa lo que dictaste".to_string(),
            },
            Status::Downloading {
                model,
                downloaded,
                total,
            } => MicUi {
                color: t.warn,
                hint_color: t.dim,
                // Requisito R4: tamaño descargado/total + tiempo restante
                // (medido con la EMA de velocidad), no solo un porcentaje.
                label: download_progress_label(
                    model.label(),
                    *downloaded,
                    *total,
                    self.dl_rate,
                ),
                hint: "Primera vez con este modelo: se descarga una sola vez".to_string(),
            },
            Status::Error(e) => MicUi {
                color: t.err,
                hint_color: t.err,
                label: "Error".to_string(),
                hint: e.clone(),
            },
        }
    }

    /// Aplica cleanup + diccionario + reglas de corrección hablada.
    /// Devuelve el texto del turno y si las REGLAS resolvieron una corrección
    /// (en ese caso el LLM no debe intervenir — nivel A de la spec §2).
    pub(crate) fn apply_pipeline(&mut self, raw: &str) -> (String, bool) {
        // Orden de procesado (igual que la web): cleanup -> diccionario.
        let mut text = raw.to_string();
        self.removed_fillers.clear();

        if self.config.cleanup_enabled() {
            let cleaned = clean_transcript_with(raw, &self.config.fillers());
            text = cleaned.text;
            self.removed_fillers = cleaned.removed;
        }

        if !self.config.dictionary.is_empty() {
            let dict = apply_dictionary(&text, &self.config.dictionary);
            text = dict.text;
        }

        // Corrección hablada multi-turno.
        let turn = self.session.accept(&text);
        if let Some(c) = &turn.correction {
            self.last_correction = Some(format!(
                "Rectificado: “{}” → “{}”",
                c.original, c.replacement
            ));
        } else if self.session.has_pending_correction() {
            self.last_correction = Some("Esperando el resto de la corrección…".to_string());
        } else if !self.last_turn.is_empty() {
            self.last_correction = None;
        }
        (turn.text, turn.correction.is_some())
    }
}

impl WhisprApp {
    /// El «reductor» de Iced: recibe cada `Message` y muta el estado.
    /// Devuelve una `Task` cuando hay que pedirle algo al runtime (abrir
    /// ventanas, salir) o `Task::none()` si el cambio es solo de estado.
    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            m @ (Message::Toggle
                | Message::StartRecording
                | Message::StopRecording
                | Message::AudioDeviceLost(..)) => self.handle_recording_control(m),
            m @ Message::TranscriptionDone(..) => self.handle_transcription_done(m),
            m @ (Message::ModelSelected(..)
                | Message::ToggleCleanup(..)
                | Message::ToggleAutoInject(..)) => self.handle_dictation_settings(m),
            m @ (Message::CorrectionProviderChanged(..)
                | Message::ToggleAiCorrection(..)
                | Message::LlmEndpointChanged(..)
                | Message::LlmModelChanged(..)
                | Message::LlmApiKeyChanged(..)
                | Message::LlmTimeoutChanged(..)
                | Message::LlmServerPathChanged(..)
                | Message::LlmServerPathBrowse
                | Message::LlmServerPathPicked(..)
                | Message::LlmServerPathReset
                | Message::LlmThresholdChanged(..)
                | Message::LlmConfirmToggled(..)
                | Message::ApplyPendingDiff
                | Message::DiscardPendingDiff
                | Message::CorrectionDone(..)
                | Message::UndoLastCorrection) => self.handle_corrector_settings(m),
            // ── Nota editable (requisito R3) ──────────────────────────────
            m @ (Message::TranscriptEdited(..)
                | Message::ClearPressed
                | Message::ClearTranscript
                | Message::UndoClearTranscript
                | Message::CopyTranscript
                | Message::InjectLast) => self.handle_note(m),
            m @ (Message::AddEntry
                | Message::EntryFromChanged(..)
                | Message::EntryToChanged(..)
                | Message::RemoveEntry(..)
                | Message::AddFiller
                | Message::FillerInputChanged(..)
                | Message::FillerCategorySelected(..)
                | Message::RemoveFiller { .. }
                | Message::ResetFillers
                | Message::ExportDictionary
                | Message::ImportDictionary) => self.handle_lists(m),
            m @ (Message::InputDeviceSelected(..)
                | Message::LanguageSelected(..)
                | Message::BeamSizeChanged(..)
                | Message::ThreadsChanged(..)) => self.handle_dictation_engine(m),
            m @ (Message::ToggleTheme
                | Message::ThemeModeChanged(..)
                | Message::FontSizeChanged(..)
                | Message::BubbleSizeChanged(..)
                | Message::BubbleCornerChanged(..)
                | Message::BubbleStyleChanged(..)
                | Message::BubbleOpacityChanged(..)
                | Message::BubbleHideTimeoutChanged(..)
                | Message::BgImageBrowse
                | Message::BgImagePicked(..)
                | Message::BgImageClear
                | Message::BubbleImageBrowse
                | Message::BubbleImagePicked(..)
                | Message::BubbleImageClear
                | Message::BubbleFramesBrowse
                | Message::BubbleFramesPicked(..)
                | Message::BubbleFramesClear
                | Message::BubbleFpsChanged(..)
                | Message::AnimationTick) => self.handle_appearance_settings(m),
            // ── Personalización II: paleta ─────────────────────────────────
            m @ (Message::PaletteFieldChanged(..)
                | Message::PaletteFieldApply(..)
                | Message::PaletteFieldReset(..)
                | Message::PaletteResetAll
                | Message::PalettePresetApply(..)
                | Message::PaletteExport
                | Message::PaletteExportPicked(..)
                | Message::PaletteImport
                | Message::PaletteImportPicked(..)
                | Message::ToggleSettings
                | Message::ToggleAdvancedSettings
                | Message::SettingsSectionSelected(..)
                | Message::WindowSectionSelected(..)) => self.handle_palette_settings(m),
            m @ Message::Poll => self.handle_poll(m),
            m @ (Message::BubbleOpened(..)
                | Message::BubblePress
                | Message::BubbleRelease
                | Message::BubbleRightPress
                | Message::BubbleRightRelease
                | Message::BubbleCursorMoved(..)
                | Message::BubbleCursorLeft) => self.handle_bubble_events(m),
            // ── Bandeja del sistema ─────────────────────────────────────────
            m @ (Message::WindowOpened(..)
                | Message::ToggleMainWindow
                | Message::FontLoaded(..)
                | Message::Startup(..)
                | Message::RetryStartup
                | Message::OpenLogFolder) => self.handle_window_and_tray(m),
            // Cierre de la ventana principal → se oculta a la bandeja (la app
            // sigue viva: burbuja, atajo global y bandeja). El cierre lo
            // gestionamos nosotros (`exit_on_close_request = false` en ambas
            // ventanas, ver `bubble::bubble_settings`).
            Message::WindowCloseRequested(id) if Some(id) == self.main_window_id => {
                self.hide_main_window()
            }
            // La burbuja no tiene botón de cierre (sin decoración): un
            // WM_CLOSE ahí solo llega desde fuera (taskkill, cierre de sesión)
            // y sí termina la app. También cubre el caso de que la principal
            // aún no se haya registrado (cierre antes de WindowOpened).
            Message::WindowCloseRequested(id) => {
                crate::log!(
                    "tray",
                    "cierre de ventana {id} → salir; terminando server LLM propio"
                );
                crate::llm::kill_launched_server();
                self.persist_config();
                self.config_save_deadline = None;
                iced::exit()
            }
        }
    }

    /// Procesa un evento del hilo de arranque (whisper + LLM). Transiciones:
    /// Loading → (WhisperFailed) → Failed · Loading → … → Ready. Cuando el
    /// arranque ya terminó (re-fase LLM en caliente por el toggle), solo
    /// actualiza `llm_status` y el aviso.
    pub(crate) fn handle_startup_event(&mut self, event: StartupEvent) -> Task<Message> {
        match event {
            StartupEvent::WhisperProgress { downloaded, total } => {
                // La velocidad estimada (EMA) se alimenta aquí: la pantalla
                // de carga y el hero comparten el mismo cálculo (R4: tamaño
                // + tiempo restante, no solo un porcentaje).
                self.note_download_progress(downloaded, total);
                self.startup_label =
                    download_progress_label("modelo de voz", downloaded, total, self.dl_rate);
                Task::none()
            }
            StartupEvent::WhisperReady => {
                crate::log!("startup", "whisper: modelo listo en cache (prerroll)");
                self.startup_label = "Preparando transcripción…".to_string();
                Task::none()
            }
            StartupEvent::WhisperFailed(e) => {
                crate::log!("startup", "whisper: fallo bloqueante: {e}");
                self.startup = StartupPhase::Failed { error: e };
                self.startup_rx = None;
                if let Some(tray) = &self.tray {
                    tray.status_item
                        .set_text(crate::tray::MENU_LABEL_STARTUP_ERROR);
                }
                Task::none()
            }
            StartupEvent::LlmStarting => {
                self.startup_label = "Preparando corrección IA…".to_string();
                Task::none()
            }
            StartupEvent::LlmReady { launched } => {
                crate::log!(
                    "startup",
                    "server LLM {}",
                    if launched {
                        "lanzado por la app"
                    } else {
                        "ya estaba activo (reutilizado)"
                    }
                );
                self.llm_status = LlmStatus::Ready { launched };
                self.startup_rx = None;
                if matches!(self.startup, StartupPhase::Loading) {
                    self.startup = StartupPhase::Ready;
                    crate::log!("startup", "arranque completo: listo para dictar");
                    self.set_tray_status(crate::tray::MENU_LABEL_READY);
                } else {
                    self.last_correction =
                        Some("Corrección IA lista (server verificado) ✓".to_string());
                }
                Task::none()
            }
            StartupEvent::LlmDisabled(reason) => {
                self.llm_status = LlmStatus::Disabled(reason.clone());
                self.startup_rx = None;
                if reason.is_empty() {
                    crate::log!(
                        "startup",
                        "fase LLM omitida: corrección IA desactivada en Ajustes"
                    );
                } else {
                    crate::log!("startup", "corrección IA desactivada: {reason}");
                }
                if matches!(self.startup, StartupPhase::Loading) {
                    self.startup = StartupPhase::Ready;
                    self.set_tray_status(crate::tray::MENU_LABEL_READY_NO_LLM);
                }
                // Aviso VISIBLE (requisito 09/09/2026: el fallo del server
                // nunca es silencioso), salvo cuando el propio usuario apagó
                // la corrección (razón vacía: no hay nada que avisar).
                if !reason.is_empty() {
                    self.last_correction = Some(format!("Corrección IA desactivada: {reason}"));
                }
                Task::none()
            }
        }
    }

    /// Libera la captura del ratón y cierra el arrastre con botón derecho
    /// (la máquina de estados ya quedó limpia en el llamador). Persiste la
    /// posición física final de la burbuja en el Config: el usuario la dejó
    /// ahí, y `apply_bubble_extras` la re-aplica en el siguiente arranque.
    /// Log con el motivo: clave para diagnosticar arrastres que terminen raro.
    pub(crate) fn end_bubble_drag(&mut self, reason: &str) {
        if let Some(drag) = self.bubble_drag.take() {
            drag.end();
        }
        // Posición física (px de pantalla) leída por winapi: manda sobre la
        // esquina configurada hasta que el usuario re-elija esquina en
        // Ajustes (BubbleCornerChanged la limpia).
        if let Some((x, y)) = crate::bubble::bubble_window_position() {
            self.config.bubble_x = Some(x);
            self.config.bubble_y = Some(y);
            self.save_config();
            crate::log!("bubble", "posición del arrastre persistida: ({x},{y})");
        }
        crate::log!("bubble", "fin de arrastre con botón derecho ({reason})");
    }

    /// Re-muestra la burbuja si el auto-ocultar la escondió, y cancela
    /// cualquier timer pendiente (dictar la mantiene visible). No-op si la
    /// burbuja ya está visible.
    pub(crate) fn show_bubble(&mut self) -> Task<Message> {
        self.bubble_hide_deadline = None;
        if !self.bubble_hidden {
            return Task::none();
        }
        self.bubble_hidden = false;
        if let Some(id) = self.bubble_id {
            return window::change_mode(id, window::Mode::Windowed);
        }
        Task::none()
    }

    /// Arma el temporizador de auto-ocultar de la burbuja (o lo cancela si el
    /// timeout es «Nunca»). Se llama al terminar un turno: cuando expira, el
    /// Poll oculta la burbuja. La actividad del usuario (hover, grabar) lo
    /// pospone o cancela.
    pub(crate) fn arm_bubble_hide_timeout(&mut self) {
        self.bubble_hide_deadline = self
            .config
            .bubble_hide_timeout()
            .secs()
            .map(|secs| std::time::Instant::now() + Duration::from_secs(secs));
    }

    /// Oculta la ventana principal (se queda en la bandeja) y actualiza el
    /// texto del ítem del menú.
    pub(crate) fn hide_main_window(&mut self) -> Task<Message> {
        let Some(id) = self.main_window_id else {
            return Task::none();
        };
        self.main_visible = false;
        crate::log!("tray", "ocultando ventana principal (queda en la bandeja)");
        if let Some(tray) = &self.tray {
            tray.toggle_item.set_text(crate::tray::MENU_LABEL_HIDDEN);
        }
        window::change_mode(id, window::Mode::Hidden)
    }

    /// Vuelve a mostrar (y enfocar) la ventana principal desde la bandeja.
    pub(crate) fn show_main_window(&mut self) -> Task<Message> {
        let Some(id) = self.main_window_id else {
            return Task::none();
        };
        self.main_visible = true;
        crate::log!("tray", "mostrando ventana principal");
        if let Some(tray) = &self.tray {
            tray.toggle_item.set_text(crate::tray::MENU_LABEL_VISIBLE);
        }
        Task::batch([
            window::change_mode(id, window::Mode::Windowed),
            window::gain_focus(id),
        ])
    }

    /// Suscripciones de Iced: el Poll de 100 ms (latido que drena canales
    /// de hilos y relojes efímeros), cierres/aperturas de ventana y los
    /// eventos de ratón de la burbuja.
    pub fn subscription(&self) -> Subscription<Message> {
        let poll = iced::time::every(Duration::from_millis(100)).map(|_| Message::Poll);
        let closes = iced::window::close_requests().map(Message::WindowCloseRequested);
        // Descubre el id de la ventana principal (iced no lo devuelve al
        // crearla desde `program::run`; la burbuja se identifica por el suyo).
        let opened = iced::window::open_events().map(Message::WindowOpened);
        let bubble = crate::bubble::bubble_events_subscription(self.bubble_id);
        // Personalización III: si hay frames resueltos para animar la
        // burbuja, añadimos un reloj que avanza el frame cada `1000/fps`
        // ms. Cuando no hay animación activa, devolvemos
        // `Subscription::none()` (iced detecta que la Subscription es
        // vacía y no la crea): cero coste extra. El `interval_from_fps`
        // cae al default si el FPS es inválido (defensa redundante: el
        // getter ya clampea).
        let animation = if self.bubble_frames_resolved().is_some() {
            let fps = self.config.bubble_animation_fps();
            let interval = crate::custom_image::interval_from_fps(fps);
            iced::time::every(interval).map(|_| Message::AnimationTick)
        } else {
            Subscription::none()
        };
        Subscription::batch([poll, closes, opened, bubble, animation])
    }

    /// Tema iced según el modo resuelto (dark por defecto, auto sigue al
    /// sistema; el Poll lo refresca cada 5 s). El `Theme` se construye con
    /// la paleta resuelta (defaults DARK/LIGHT + overrides del usuario), así
    /// los 5 campos del `Palette` de iced (fondo de ventana, texto primario,
    /// acento, ok, err) también respetan la personalización.
    pub fn theme(&self) -> Theme {
        let palette = theme::resolve_palette(
            &self.config,
            self.config.resolved_dark(self.system_dark),
        );
        theme::custom_with_palette(&palette)
    }

    /// Paleta resuelta (con overrides aplicados) para el modo activo.
    /// Pensada para que `view()` la use explícitamente al pintar textos y
    /// tarjetas (los widgets con estilo siguen leyendo `tokens(theme)`,
    /// que a su vez consulta el thread-local del override).
    pub(crate) fn resolved_palette(&self) -> theme::UiPalette {
        theme::resolve_palette(
            &self.config,
            self.config.resolved_dark(self.system_dark),
        )
    }

    /// Dibuja la ventana principal: pantalla de carga hasta que el arranque
    /// termine; después, header + chip de corrección IA + hero del
    /// micrófono + nota editable + acciones + feedback + pie de confianza
    /// (o el panel de Ajustes en la zona central).
    pub fn view(&self) -> Element<'_, Message> {
        // R7-II: instala la paleta resuelta en el thread-local de `theme` para
        // que los style closures de los widgets (`primary`, `secondary`, …)
        // lean los overrides del usuario sin tocar sus firmas. El scope se
        // libera al volver (Drop) ⇒ el siguiente frame empieza limpio.
        let _palette_scope = theme::PaletteOverrideScope::new(self.resolved_palette());
        let t = theme::tokens(&self.theme()).into_owned();
        // Tamaño de fuente global resuelto (Ajustes → Apariencia): TODOS los
        // textos de la vista lo usan (fs.*) en vez de los literales FONT_*.
        let fs = theme::font_sizes(self.config.font_size().factor());

        // Pantalla de carga (requisito UX 09/09/2026): mientras el arranque
        // no termine se muestra SOLO spinner + fase, sin UI de dictado.
        // Whisper y el server LLM se cargan en background (hilo propio).
        if !matches!(self.startup, StartupPhase::Ready) {
            return self.loading_view(&t, fs);
        }

        // ── Cabecera: wordmark + controles globales ──────────────────────────
        // El botón de tema muestra el modo EFECTIVO (Auto incluido) y cicla
        // Auto → Claro → Oscuro al pulsarlo.
        let theme_toggle = match self.config.theme_mode() {
            ThemeMode::Auto => "◐ Auto",
            ThemeMode::Claro => "☀ Claro",
            ThemeMode::Oscuro => "☾ Oscuro",
        };
        // El botón «Ajustes ▸» de la cabecera desaparece: abrir Ajustes es
        // ahora una PESTAÑA de la ventana (barra de secciones, abajo).
        let header = row![
            text("Whispr Open")
                .size(fs.xl)
                .font(Font::MONOSPACE)
                .color(t.text),
            Space::with_width(Length::Fill),
            secondary_button(theme_toggle).on_press(Message::ToggleTheme),
        ]
        .spacing(theme::SPACE_2)
        .align_y(iced::Alignment::Center);

        // ── Secciones de la VENTANA (UI 24/09/2026) ────────────────────────
        // Mismo lenguaje que Ajustes: la fila de pestañas va ARRIBA y la
        // sección activa se marca con `●` además del color. Dos secciones,
        // una pregunta cada una: «Dictado» (hero + nota) y «Ajustes» (panel).
        let window_section = self.window_section();
        let window_tabs = crate::ui::tabs::tab_bar(
            WindowSection::ALL
                .iter()
                .map(|section| {
                    crate::ui::tabs::Tab::new(
                        section.label(),
                        Message::WindowSectionSelected(*section),
                        window_section == *section,
                    )
                })
                .collect(),
            fs,
        );

        // ── Estado del corrector IA (chip) ─────────────────────────────────
        // Transparencia (requisito UX 10/09/2026): el usuario ve SIEMPRE si
        // al dictar puede actuar la IA correctora y con qué modelo. Verde =
        // activa, ámbar = se dicta sin corregir (motivo visible), gris =
        // apagada a propósito o aún comprobando. Misma lógica pura que la
        // línea de estado de Ajustes (`llm_status_ui`).
        let llm_chip = {
            let ui = llm_status_ui(
                self.config.ai_correction_enabled(),
                &self.llm_status,
                &self.config.llm_settings().model,
            );
            let color = match ui.kind {
                "ok" => t.ok,
                "warn" => t.warn,
                _ => t.dim,
            };
            ui_text(ui.label).size(fs.sm).color(color)
        };

        // ── Hero: estado del micrófono (tira horizontal compacta) ────────────
        // En portrait el hero deja de ser una columna centrada y pasa a una
        // fila: punto de estado + label + hint a la derecha; la barra de
        // nivel/progreso va debajo, a todo el ancho de la tarjeta.
        // Feedback doble intacto: color de estado (punto + borde) y label.
        let mic = self.mic_ui(&t);
        let mut hero_content = column![row![
            text("●").size(fs.xl).color(mic.color),
            column![
                text(mic.label).size(fs.hero).color(t.text),
                text(mic.hint).size(fs.sm).color(mic.hint_color),
            ]
            .width(Length::Fill)
            .spacing(theme::SPACE_1),
        ]
        .spacing(theme::SPACE_3)
        .align_y(iced::Alignment::Center),]
        .spacing(theme::SPACE_2);

        // Micro-feedback (>= 200 ms) no decorativo: nivel del mic / progreso.
        match &self.status {
            Status::Recording => {
                let level = self.mic_level_percent();
                hero_content = hero_content.push(
                    progress_bar(0.0..=100.0, level as f32)
                        .width(Length::Fill)
                        .style(move |th| theme::progress(th, t.accent)),
                );
                // Pico del turno (UI-FUTURA P2): la barra dibuja el PICO que
                // retiene `AudioCapture`, así que se etiqueta como tal y se
                // avisa cuando el micro va mudo o saturado. Se pinta con
                // tokens del tema (funciona igual en claro y oscuro).
                let mut fila = row![
                    text(format!("pico {level}%")).size(fs.xxs).color(t.dim),
                    Space::with_width(Length::Fill),
                ]
                .align_y(iced::Alignment::Center);
                if let Some(hint) = self.mic_peak_hint() {
                    fila = fila.push(ui_text(hint.label()).size(fs.xxs).color(t.warn));
                }
                hero_content = hero_content.push(fila);
            }
            Status::Downloading {
                downloaded, total, ..
            } if *total > 0 => {
                let pct = (*downloaded as f32 / *total as f32 * 100.0).min(100.0);
                hero_content = hero_content.push(
                    progress_bar(0.0..=100.0, pct)
                        .width(Length::Fill)
                        .style(move |th| theme::progress(th, t.warn)),
                );
            }
            _ => {}
        }

        let hero = button(
            container(hero_content)
                .width(Length::Fill)
                .padding(theme::SPACE_3)
                .style(move |th| theme::mic_card(th, mic.color)),
        )
        .on_press(Message::Toggle)
        .style(theme::mic);

        // ── Transcript: editor multilínea (requisito R3) ─────────────────────
        // La nota se puede corregir a mano: un error de Whisper ya no
        // obliga a re-dictar. El editor gestiona su propio scroll (sige al
        // cursor); el dictado nuevo llega con el cursor al final
        // (`sync_editor_from_transcript`).
        let transcript_card = container(
            text_editor(&self.transcript_editor)
                .placeholder(self.placeholder_text.as_str())
                .on_action(Message::TranscriptEdited)
                .size(fs.md)
                .padding(theme::SPACE_3)
                .height(Length::Fill)
                .style(theme::text_editor),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .style(theme::flat);

        // ── Acciones discretas ───────────────────────────────────────────────
        let inject_label = if self.config.auto_inject.unwrap_or(true) {
            "Otra vez"
        } else {
            "Escribir en app"
        };
        // Confirmación visual breve del copiado (requisito R3): «✓ Copiado»
        // ~2 s en el propio botón (caduca en el Poll).
        let copy_label: &str = if self.copied_at.is_some() {
            "✓ Copiado"
        } else {
            "Copiar"
        };
        // Borrado explícito y reversible (requisito R3): primer clic ARMA
        // la confirmación; el borrado real («Sí, borrar») lleva backup para
        // deshacer. La confirmación caduca a los 5 s.
        let clear_controls: Element<'_, Message> = if self.clear_armed {
            row![
                text("¿Borrar la nota?").size(fs.sm).color(t.warn),
                button(ui_text("Sí, borrar"))
                    .on_press(Message::ClearTranscript)
                    .padding(theme::SECONDARY_BUTTON_PADDING)
                    .style(theme::primary),
                secondary_button("No").on_press(Message::ClearPressed),
            ]
            .spacing(theme::SPACE_2)
            .align_y(iced::Alignment::Center)
            .into()
        } else {
            secondary_button("Limpiar")
                .on_press(Message::ClearPressed)
                .into()
        };
        let actions = row![
            secondary_button(copy_label).on_press(Message::CopyTranscript),
            clear_controls,
            if self.last_turn.is_empty() {
                secondary_button(inject_label)
            } else {
                secondary_button(inject_label).on_press(Message::InjectLast)
            },
        ]
        .spacing(theme::SPACE_2)
        .align_y(iced::Alignment::Center);

        // ── Feedback textual ────────────────────────────────────────────────
        // Con undo disponible, el feedback lleva el botón «Deshacer» (spec
        // §4.3: AI transparente y overrideable — un nivel de undo basta).
        // Con diff pendiente (modo confirmar), lleva «Aplicar» / «Descartar».
        let feedback: Element<'_, Message> = if self.pending_diff.is_some() {
            // Modo confirmar activo: el diff está esperando decisión.
            // Mantenemos el texto del last_correction (puesto por
            // handle_llm_correction) y añadimos los dos botones.
            let label = self
                .last_correction
                .as_deref()
                .unwrap_or("Corrección IA pendiente de confirmación");
            row![
                text(label)
                    .size(fs.sm)
                    .color(t.warn)
                    .shaping(Shaping::Advanced),
                button(ui_text("Aplicar"))
                    .on_press(Message::ApplyPendingDiff)
                    .padding(theme::SECONDARY_BUTTON_PADDING)
                    .style(theme::primary),
                secondary_button("Descartar").on_press(Message::DiscardPendingDiff),
            ]
            .spacing(theme::SPACE_2)
            .align_y(iced::Alignment::Center)
            .into()
        } else if let Some(corr) = &self.last_correction {
            let color = if feedback_is_warning(corr) {
                t.warn
            } else {
                t.ok
            };
            let mut row_el = row![text(corr.as_str())
                .size(fs.sm)
                .color(color)
                // El texto puede traer glifos de UI (↩ de «deshecha ↩»):
                // Advanced activa el fallback de fuente.
                .shaping(Shaping::Advanced)]
            .spacing(theme::SPACE_2)
            .align_y(iced::Alignment::Center);
            if self.undo_state.is_some() {
                row_el = row_el.push(
                    secondary_button("↩ Deshacer").on_press(Message::UndoLastCorrection),
                );
            }
            // Reversibilidad del borrado (requisito R3): junto al aviso
            // «Nota borrada» vive el deshacer específico del borrado.
            if self.cleared_backup.is_some() {
                row_el = row_el.push(
                    secondary_button("↩ Deshacer borrado")
                        .on_press(Message::UndoClearTranscript),
                );
            }
            row_el.into()
        } else if !self.removed_fillers.is_empty() {
            text(format!(
                "Muletillas eliminadas: {}",
                self.removed_fillers.join(", ")
            ))
            .size(fs.sm)
            .color(t.dim)
            .into()
        } else {
            text("").into()
        };

        // ── Cuerpo de la sección activa ────────────────────────────────────
        // «Dictado» reúne lo del turno (chip del corrector, hero, nota y sus
        // acciones) en una columna; «Ajustes» es el panel de siempre con su
        // propio scroll. La nota conserva su scroll independiente y no se
        // empuja hacia abajo al abrir Ajustes.
        let section_body: Element<'_, Message> = if self.settings_open {
            scrollable(self.settings_view(&t))
                .style(theme::scrollable)
                .height(Length::Fill)
                .into()
        } else {
            let mut dictado = column![llm_chip, hero, transcript_card]
                .spacing(theme::SPACE_3)
                .height(Length::Fill);
            // Las acciones y el feedback operan sobre la nota: van con ella.
            dictado = dictado.push(actions).push(feedback);
            dictado.into()
        };

        // ── Pie de confianza (requisito R4): siempre visible, discreto ─────
        // Promesa HONESTA: la transcripción es siempre local, pero si el
        // usuario activó la corrección en la nube, el texto corregido SÍ
        // sale (así lo elige y así se le dice — jamás una promesa falsa).
        let trust_line = trust_footer_line(
            self.config.ai_correction_enabled()
                && matches!(
                    self.config.correction_provider(),
                    CorrectionProvider::Cloud
                ),
        );
        let footer = container(text(trust_line).size(fs.xxs).color(t.dim))
            .width(Length::Fill)
            .align_x(iced::alignment::Horizontal::Center)
            .padding(theme::SPACE_1);

        let mut content = column![header, window_tabs, section_body]
            .spacing(theme::SPACE_3)
            .padding(theme::SPACE_3);
        // El pie de confianza va SIEMPRE (también dentro de Ajustes): la
        // promesa de privacidad no desaparece al abrir un panel.
        content = content.push(footer);

        // Personalización III: si el usuario eligió una imagen de fondo,
        // se apila DEBAJO del contenido como capa de fondo (stack con la
        // columna actual). El contenedor usa `Length::Fill`/`Length::Fill`
        // para ocupar la ventana completa; la imagen se estira para
        // cubrir (ContentFit::Cover, el equivalente visual de un fondo
        // CSS cover).
        // H7a (QA 24/09/2026): una sola consulta a `bg_image()`, resuelta con
        // `match`. Antes se preguntaba `is_some()` y luego se hacía
        // `expect(...)`: dos validaciones del fichero (la ruta se revalida en
        // cada llamada, así que puede desaparecer entre ambas) y un pánico
        // evitable en código de UI.
        match self.bg_image() {
            Some(path) => {
                // Stack: la imagen va al fondo, el contenido al frente.
                // iced expone `Stack` como helper que apila elementos sin
                // espaciado y respeta el orden de inserción: el primero es
                // el del fondo, el último el del frente. El handle se
                // construye dentro del brazo para que el borrow del PathBuf
                // quede dentro del scope.
                let handle = iced::widget::image::Handle::from_path(&path);
                let bg_layer: Element<'_, Message> = container(
                    iced::widget::image(handle)
                        .content_fit(iced::ContentFit::Cover)
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .opacity(1.0),
                )
                .width(Length::Fill)
                .height(Length::Fill)
                .into();
                let foreground: Element<'_, Message> = container(content)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into();
                iced::widget::stack![bg_layer, foreground].into()
            }
            None => container(content)
                .width(Length::Fill)
                .height(Length::Fill)
                .style(|th| container::Style {
                    background: Some(Background::Color(theme::tokens(&th).bg)),
                    ..Default::default()
                })
                .into(),
        }
    }

    /// Pantalla de carga sencilla (requisito UX 09/09/2026): spinner
    /// animado + texto de la fase en curso. Cuando la fase es Failed se
    /// muestra el motivo y «Reintentar» (que relanza el prerroll completo;
    /// si el modelo de voz faltaba, la descarga arranca sola). El dictado
    /// queda bloqueado hasta que `startup` sea Ready.
    fn loading_view(&self, t: &UiPalette, fs: theme::FontSizes) -> Element<'_, Message> {
        // Spinner de 4 fases, animado por `startup_tick` (el Poll lo avanza
        // a 10 Hz mientras la fase sea Loading).
        const SPINNER: [&str; 4] = ["◐", "◓", "◑", "◒"];
        let spin = SPINNER[(self.startup_tick % 4) as usize];

        let mut content = column![
            ui_text(spin).size(fs.hero).color(t.accent),
            text("Cargando…").size(fs.xl).color(t.text),
            text(self.startup_label.as_str()).size(fs.sm).color(t.dim),
        ]
        .spacing(theme::SPACE_3)
        .align_x(iced::Alignment::Center)
        .width(Length::Fill);

        if let StartupPhase::Failed { error } = &self.startup {
            content = content.push(
                text(format!(
                    "{error} — si falta el modelo de voz, «Reintentar» lo descarga \
                     automáticamente en %APPDATA%\\whispr-open\\models."
                ))
                .size(fs.sm)
                .color(t.err)
                .shaping(Shaping::Advanced),
            );
            content = content.push(
                button(ui_text("Reintentar"))
                    .on_press(Message::RetryStartup)
                    .style(theme::primary),
            );
        }

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|th| container::Style {
                background: Some(Background::Color(theme::tokens(&th).bg)),
                ..Default::default()
            })
            .into()
    }
}

/// Pestaña activa del panel de Ajustes (R7 — Personalización I). Cada
/// variante corresponde a un grupo de controles del antiguo `settings_view`
/// monolítico: dividir el panel en pestañas quita el ruido que vivía el
/// usuario al ver toda la configuración apilada en una sola vista larga con
/// scroll infinito.
///
/// El estado vive solo en memoria (`WhisprApp::settings_section`); NO se
/// persiste en `config.json` porque la regla del producto es que la última
/// sección abierta solo se recuerda dentro de la misma sesión de la app.
///
/// Notas de diseño:
/// - El orden del enum es el orden visual de las pestañas y el del
///   ciclo `next()` / `previous()` (por si en el futuro se añade
///   navegación por teclado).
/// - Cada variante tiene una etiqueta legible en español para que las
///   pestañas sean accesibles también sin color (requisito 4 de la
///   tarea R7).
/// - La pestaña activa se marca con fondo + borde inferior y un
///   indicador `●` antes de la etiqueta, no solo con color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsSection {
    General,
    Apariencia,
    Dictado,
    CorreccionIA,
    Avanzado,
}

impl SettingsSection {
    /// Orden de las pestañas, igual al del enum.
    pub(crate) const ALL: [SettingsSection; 5] = [
        SettingsSection::General,
        SettingsSection::Apariencia,
        SettingsSection::Dictado,
        SettingsSection::CorreccionIA,
        SettingsSection::Avanzado,
    ];

    /// Etiqueta legible en español (requisito 4: texto, no solo icono).
    pub(crate) fn label(self) -> &'static str {
        match self {
            SettingsSection::General => "General",
            SettingsSection::Apariencia => "Apariencia",
            SettingsSection::Dictado => "Dictado",
            SettingsSection::CorreccionIA => "Corrección IA",
            SettingsSection::Avanzado => "Avanzado",
        }
    }
}

impl std::fmt::Display for SettingsSection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// Aviso del medidor del micrófono (UI-FUTURA P2). El nivel que se dibuja
/// es el PICO que retiene `AudioCapture` durante el turno, así que el aviso
/// es honesto: dice «pico» y solo aparece cuando aporta algo (micro mudo o
/// saturado). Texto, no solo color (accesibilidad).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MicPeakHint {
    /// Sin señal apreciable pasado el margen inicial del turno.
    Silent,
    /// Pico al máximo: el micro puede estar saturando.
    Clipping,
}

impl MicPeakHint {
    /// Frase accionable para el usuario (en su idioma).
    pub(crate) fn label(self) -> &'static str {
        match self {
            MicPeakHint::Silent => "sin señal del micrófono: ¿está silenciado?",
            MicPeakHint::Clipping => "pico al máximo: el micrófono puede estar saturando",
        }
    }
}

/// Pico (%) a partir del cual se avisa de saturación.
pub(crate) const MIC_CLIP_PERCENT: u32 = 97;
/// Pico (%) por debajo del cual, pasados `MIC_SILENT_SECS`, se avisa de mudo.
pub(crate) const MIC_SILENT_PERCENT: u32 = 2;
/// Segundos mínimos de turno antes de acusar «sin señal» (evita el aviso
/// parpadeante del primer instante de la grabación).
pub(crate) const MIC_SILENT_SECS: u64 = 2;

/// Decide el aviso del medidor a partir del pico y de los segundos de
/// turno. Función pura (sin estado) para que los tests la cubran.
pub(crate) fn mic_peak_hint(level_percent: u32, recording_secs: u64) -> Option<MicPeakHint> {
    if level_percent >= MIC_CLIP_PERCENT {
        Some(MicPeakHint::Clipping)
    } else if recording_secs >= MIC_SILENT_SECS && level_percent < MIC_SILENT_PERCENT {
        Some(MicPeakHint::Silent)
    } else {
        None
    }
}

/// Sección visible de la VENTANA PRINCIPAL (UI 24/09/2026).
///
/// La ventana principal era un hero del micrófono + transcript + controles
/// sueltos; Ajustes ya tenía pestañas. Ahora las dos superficies hablan el
/// mismo idioma con dos secciones y una sola pregunta cada una:
///
/// - `Dictado`: ¿qué estoy dictando? (hero, estado, medidor, nota y acciones).
/// - `Ajustes`: ¿cómo quiero que funcione? (el panel que ya existía).
///
/// El estado NO se duplica: `settings_open` sigue siendo la única fuente de
/// verdad ([`WhisprApp::window_section`] lo lee) para no tocar los handlers de
/// Ajustes ni su lógica de persistencia.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowSection {
    Dictado,
    Ajustes,
}

impl WindowSection {
    /// Orden visual de las pestañas.
    pub(crate) const ALL: [WindowSection; 2] = [WindowSection::Dictado, WindowSection::Ajustes];

    /// Etiqueta legible en español (texto, no solo icono).
    pub(crate) fn label(self) -> &'static str {
        match self {
            WindowSection::Dictado => "Dictado",
            WindowSection::Ajustes => "Ajustes",
        }
    }
}

impl std::fmt::Display for WindowSection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// Categoría de una muletilla en Ajustes. Cada variante corresponde a una
/// regla distinta de `cleanup.rs`: inequívoca (cualquier posición), solo
/// antes de pausa, o solo abriendo el turno. Se usa en el `pick_list` de
/// Ajustes y en los mensajes de añadir/quitar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FillerCategory {
    Unambiguous,
    Ambiguous,
    StartOfTurn,
}

impl FillerCategory {
    pub(crate) const ALL: [FillerCategory; 3] = [
        FillerCategory::Unambiguous,
        FillerCategory::Ambiguous,
        FillerCategory::StartOfTurn,
    ];

    fn label(self) -> &'static str {
        match self {
            FillerCategory::Unambiguous => "En cualquier posición",
            FillerCategory::Ambiguous => "Solo antes de pausa",
            FillerCategory::StartOfTurn => "Solo al inicio",
        }
    }

    /// Lista de palabras de esta categoría (solo lectura).
    fn list<'a>(self, fillers: &'a FillerWords) -> &'a [String] {
        match self {
            FillerCategory::Unambiguous => &fillers.unambiguous,
            FillerCategory::Ambiguous => &fillers.ambiguous,
            FillerCategory::StartOfTurn => &fillers.start_of_turn,
        }
    }

    /// Lista de palabras de esta categoría (para mutar).
    pub(crate) fn list_mut<'a>(self, fillers: &'a mut FillerWords) -> &'a mut Vec<String> {
        match self {
            FillerCategory::Unambiguous => &mut fillers.unambiguous,
            FillerCategory::Ambiguous => &mut fillers.ambiguous,
            FillerCategory::StartOfTurn => &mut fillers.start_of_turn,
        }
    }
}

impl std::fmt::Display for FillerCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// Texto de UI con [`Shaping::Advanced`]: los glifos de la interfaz
/// (☀ ☾ ▸ ▾ ✕ ↩) NO existen en Segoe UI (la SansSerif default de Windows)
/// y `Shaping::Basic` (el default de iced) no hace fallback de fuente →
/// cosmic-text devuelve glyph_id=0 (caja vacía `.notdef`). `Advanced`
/// activa el fallback por codepoint contra Segoe UI Emoji, cargada al
/// arranque en `fonts.rs`.
pub(crate) fn ui_text<'a>(content: impl Into<String>) -> Text<'a> {
    text(content.into()).shaping(Shaping::Advanced)
}

/// Botón secundario de la UI con el padding táctil del tema (UI-FUTURA P1:
/// los targets de los botones secundarios se quedaban cortos). Evita
/// repetir `button(ui_text(..)).padding(...)` por toda la vista.
pub(crate) fn secondary_button<'a>(label: impl Into<String>) -> Button<'a, Message> {
    button(ui_text(label))
        .padding(theme::SECONDARY_BUTTON_PADDING)
        .style(theme::secondary)
}

/// Placeholder del campo «Server LLM» de Ajustes: muestra la ruta que se
/// resolvería AHORA MISMO si el usuario la dejara vacía (autodetección), o
/// un aviso si no hay ningún candidato. Informativo: no valida nada.
pub(crate) fn auto_server_placeholder(cfg: &Config) -> String {
    match crate::llm::resolve_server_path(cfg) {
        Some(p) => format!("auto: {}", p.display()),
        None => {
            "auto: no encontrado — usa «Examinar…» o un endpoint".to_string()
        }
    }
}

/// Lectura del estado del corrector IA para la UI (chip del estado principal
/// + línea de estado en Ajustes). Función pura y testeable: la transparencia
/// sobre CUÁNDO se usa el LLM es un requisito UX (10/09/2026), así que la
/// lógica de etiquetas no puede vivir solo dentro de la vista.
///
/// Estados visibles (ejemplos del requisito: «Corrección IA: activa con
/// <modelo>» · «inactiva — servidor no encontrado»):
/// - toggle apagado     → `dim`, invita a activarla (sin prometer modelo).
/// - `Ready`            → `ok`, con el modelo en uso.
/// - `Disabled(motivo)` → `warn`, motivo visible (nunca silencioso).
/// - `Unknown`          → `dim`, «comprobando…» (arranque en curso).
pub(crate) struct LlmStatusUi {
    pub(crate) kind: &'static str,
    pub(crate) label: String,
}

pub(crate) fn llm_status_ui(enabled: bool, status: &LlmStatus, model: &str) -> LlmStatusUi {
    if !enabled {
        return LlmStatusUi {
            kind: "dim",
            label: "○ Corrección IA: apagada — actívala en Ajustes".to_string(),
        };
    }
    match status {
        LlmStatus::Ready { .. } => LlmStatusUi {
            kind: "ok",
            label: format!("● Corrección IA: activa · {model}"),
        },
        LlmStatus::Disabled(reason) if !reason.is_empty() => LlmStatusUi {
            kind: "warn",
            label: format!("○ Corrección IA: inactiva — {reason}"),
        },
        _ => LlmStatusUi {
            kind: "dim",
            label: "○ Corrección IA: comprobando…".to_string(),
        },
    }
}

/// ¿La línea de feedback (`last_correction`) es un AVISO (ámbar) o una
/// confirmación (verde)? Función pura y testeada: la clasificación decide
/// el color que el usuario ve, así que no puede vivir escondida en la
/// vista. Avisos: esperas, problemas del corrector, micro desconectado,
/// turnos vacíos y el tope de grabación.
fn feedback_is_warning(feedback: &str) -> bool {
    feedback.starts_with("Esperando")
        || feedback.contains("no disponible")
        || feedback.contains("no sé qué corregir")
        || feedback.contains("No se oyó nada")
        || feedback.contains("micrófono")
        || feedback.contains("Dictado larguísimo")
}

/// Último turno a partir de la nota editada a mano (requisito R3): la
/// última línea con contenido. El dictado añade al final, así que «lo
/// último que vio/tocó el usuario» es la cola de la nota; si partió líneas
/// a mano, la última línea ES lo último que dictó o corrigió.
pub(crate) fn last_turn_from_transcript(transcript: &str) -> String {
    transcript
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
        .trim()
        .to_string()
}

/// Etiqueta completa del estado de descarga (requisito R4: tamaño
/// descargado/total y tiempo restante, no solo un porcentaje). La usan el
/// hero de la ventana principal y la pantalla de carga.
fn download_progress_label(
    what: &str,
    downloaded: u64,
    total: u64,
    rate: Option<f64>,
) -> String {
    let mb = |b: u64| b / (1024 * 1024);
    if total == 0 {
        return format!("Descargando {what}… (tamaño desconocido)");
    }
    format!(
        "Descargando {what}… {} / {} MB · {}",
        mb(downloaded),
        mb(total).max(1),
        download_eta_text(downloaded, total, rate)
    )
}

/// Tiempo restante legible a partir de la velocidad estimada. Sin velocidad
/// fiable → «calculando tiempo…» (nada de un «∞ s» técnico). Formatos
/// elegidos para no prometer exactitud donde no la hay: a partir de 90 min
/// se dice «más de 1 h».
fn download_eta_text(downloaded: u64, total: u64, rate: Option<f64>) -> String {
    // Menos de 1 KB/s no es una velocidad: es el arranque de la conexión.
    let Some(rate) = rate.filter(|r| *r > 1024.0) else {
        return "calculando tiempo…".to_string();
    };
    let remaining = total.saturating_sub(downloaded) as f64;
    let secs = remaining / rate;
    if secs >= 5400.0 {
        "quedan más de 1 h".to_string()
    } else if secs >= 90.0 {
        format!("quedan ~{} min", (secs / 60.0).round() as u64)
    } else if secs >= 10.0 {
        format!("quedan ~{} s", secs.round() as u64)
    } else {
        "casi listo".to_string()
    }
}

    /// Placeholder del editor cuando la nota está vacía: cómo empezar. Se
    /// guarda como campo (la API del editor pide un préstamo por el frame,
    /// no un owned).
    fn transcript_placeholder(hotkey_label: &str) -> String {
        format!(
            "Pulsa {hotkey_label} o el botón del micrófono y empieza a hablar.\n\
             La nota se puede corregir a mano aquí."
        )
    }

/// Línea del pie de confianza (requisito R4). HONESTA por encima de todo:
/// con la corrección en la nube activada, el texto corregido SÍ sale del
/// PC — y el pie lo dice en vez de prometer una privacidad falsa.
fn trust_footer_line(cloud_correction_active: bool) -> &'static str {
    if cloud_correction_active {
        "Transcripción local · la corrección IA usa la nube"
    } else {
        "Transcripción local · nada sale de tu PC"
    }
}

/// Etiqueta legible del idioma para el `pick_list`. La opción "auto"
/// representa autodetección; cualquier valor en `Option<String>` (incluido
/// None → default "es") se mapea a la cadena que el `pick_list` espera.
/// Si el usuario metió un idioma raro (p. ej. "klingon") no presente en
/// `LANGUAGE_OPTIONS`, lo mostramos igualmente para que no se "pierda".
pub(crate) fn language_label(value: &Option<String>) -> String {
    match value.as_deref().map(str::trim) {
        None => "es".to_string(),
        Some(s) if s.is_empty() => "es".to_string(),
        Some(s) if s.eq_ignore_ascii_case("auto") => "auto".to_string(),
        Some(s) => s.to_string(),
    }
}

/// Hint contextual bajo el selector de idioma: aclara lo que hace cada
/// opción. Solo aparece cuando el idioma está en "auto" o no se reconoce.
pub(crate) fn language_hint(value: &Option<String>) -> String {
    match value.as_deref().map(str::trim) {
        Some(s) if s.eq_ignore_ascii_case("auto") => "whisper decide".to_string(),
        Some(s) if s.is_empty() => "default: es".to_string(),
        None => "default: es".to_string(),
        Some(_) => "".to_string(),
    }
}

pub(crate) fn mic_pick_list(
    devices: &[String],
    selected: Option<&str>,
    fs: theme::FontSizes,
) -> Element<'static, Message> {
    if devices.is_empty() {
        return text("(sin micrófonos detectados)").size(fs.sm).into();
    }
    pick_list(
        devices.to_vec(),
        selected.map(|s| s.to_string()),
        Message::InputDeviceSelected,
    )
    .placeholder("(por defecto)")
    // Los nombres de dispositivos son largos: el selector se ajusta al
    // ancho disponible (critical en portrait).
    .width(Length::Fill)
    .style(theme::pick_list)
    .into()
}

pub(crate) fn dictionary_list(entries: &[DictionaryEntry], fs: theme::FontSizes) -> Element<'static, Message> {
    if entries.is_empty() {
        return text("Sin entradas todavía. Añade jerga, nombres o siglas.")
            .size(fs.sm)
            .into();
    }
    let mut col = Column::new().spacing(theme::SPACE_1);
    for (idx, entry) in entries.iter().enumerate() {
        col = col.push(
            row![
                text(format!("{} → {}", entry.from, entry.to)).size(fs.md),
                button(ui_text("✕"))
                    .on_press(Message::RemoveEntry(idx))
                    .style(theme::secondary),
            ]
            .spacing(theme::SPACE_2)
            .align_y(iced::Alignment::Center),
        );
    }
    col.into()
}

/// Listas de muletillas por categoría con botón ✕ para quitar (idéntico en
/// estilo a `dictionary_list`). Solo muestra las categorías con palabras;
/// si todas están vacías (el usuario quitó todo) avisa de que no se limpia
/// nada y que «Por defecto» restaura las listas de serie.
pub(crate) fn filler_lists(fillers: &FillerWords, fs: theme::FontSizes) -> Element<'static, Message> {
    if FillerCategory::ALL
        .iter()
        .all(|category| category.list(fillers).is_empty())
    {
        return text("Sin muletillas: no se limpiará nada. «Por defecto» restaura las de serie.")
            .size(fs.sm)
            .into();
    }
    let mut col = Column::new().spacing(theme::SPACE_2);
    for category in FillerCategory::ALL {
        let words = category.list(fillers);
        if words.is_empty() {
            continue;
        }
        col = col.push(text(format!("{}:", category.label())).size(fs.xxs));
        for (idx, word) in words.iter().enumerate() {
            col = col.push(
                row![
                    text(word.clone()).size(fs.md),
                    button(ui_text("✕"))
                        .on_press(Message::RemoveFiller {
                            category,
                            index: idx,
                        })
                        .style(theme::secondary),
                ]
                .spacing(theme::SPACE_2)
                .align_y(iced::Alignment::Center),
            );
        }
    }
    col.into()
}

/// Transcribe en un hilo de trabajo: a) descarga el modelo si falta (la
/// primera vez, silencioso), b) carga el contexto whisper.cpp, c) transcribe
/// con el diccionario del usuario como initial_prompt (sesgo léxico).
///
/// `inference` agrupa los parámetros ajustados en Ajustes (idioma, beam
/// size, threads). El idioma `None` significa autodetección: no se llama
/// a `set_language` y whisper decide.
fn transcribe_samples(
    samples: Vec<f32>,
    model: Model,
    dictionary: Vec<DictionaryEntry>,
    inference: crate::transcribe::InferenceParams,
) -> Result<String, String> {
    if !crate::transcribe::model_exists(model) {
        crate::log!(
            "transcribe",
            "modelo {} no está descargado, descargando…",
            model.id()
        );
        crate::transcribe::ensure_model(model, |_, _| {})
            .map_err(|e| download_failure_message(model, &e))?;
    }
    let prompt = crate::transcribe::build_initial_prompt(&dictionary);
    crate::transcribe::transcribe_cached(&samples, model, &inference, prompt.as_deref())
}

/// Mensaje de fallo de descarga para `Status::Error`: causa clasificada y
/// accionable (requisito de robustez), con el detalle técnico solo en el
/// log — nunca la cadena cruda de ureq/fs en la cara del usuario.
pub(crate) fn download_failure_message(model: Model, raw: &str) -> String {
    let hint = crate::transcribe::friendly_download_error(raw);
    crate::log!("transcribe", "fallo descargando {}: {raw}", model.id());
    format!(
        "No se pudo descargar el modelo de voz ({}): {}. \
         Revisa la conexión o el disco y reintenta.",
        model.label(),
        hint
    )
}

// ---------------------------------------------------------------------------
// Arranque en background (requisito UX 09/09/2026: «la app levanta todo por
// sí misma»). Estos hilos NO tocan la UI: solo emiten eventos por canal y el
// Poll los aplica. Terminan SIEMPRE con un evento terminal (WhisperFailed,
// LlmReady o LlmDisabled) para que la app nunca se quede colgada «cargando».
// ---------------------------------------------------------------------------

/// Fase whisper del arranque: descarga el modelo si falta (con progreso) y
/// lo carga en el cache de contexto (prerroll: la primera pulsación del
/// atajo no paga la carga). Luego delega en `run_llm_phase` si aplica.
pub(crate) fn run_startup_thread(cfg: Config, tx: Sender<StartupEvent>, cancel: Arc<AtomicBool>) {
    let model = cfg.selected_model();
    if !crate::transcribe::model_exists(model) {
        crate::log!(
            "startup",
            "modelo de voz {} ausente en %APPDATA% — descargando…",
            model.id()
        );
    }
    let path = match crate::transcribe::ensure_model(model, |downloaded, total| {
        let _ = tx.send(StartupEvent::WhisperProgress { downloaded, total });
    }) {
        Ok(path) => path,
        Err(e) => {
            crate::log!("startup", "no se pudo descargar el modelo de voz: {e}");
            let _ = tx.send(StartupEvent::WhisperFailed(format!(
                "No se pudo descargar el modelo de voz ({}). Revisa la conexión y usa «Reintentar».",
                model.label()
            )));
            return;
        }
    };
    crate::log!("startup", "modelo de voz en: {}", path.display());
    match preload_with_recovery(model, &tx) {
        Ok(()) => {
            let _ = tx.send(StartupEvent::WhisperReady);
            run_llm_phase(cfg, tx, cancel);
        }
        Err(e) => {
            crate::log!("startup", "no se pudo cargar el contexto whisper: {e}");
            let _ = tx.send(StartupEvent::WhisperFailed(format!(
                "No se pudo cargar el modelo de voz {}: {e}",
                model.label()
            )));
        }
    }
}

/// Carga el contexto whisper del modelo con UNA recuperación ante fichero
/// dañado: si el modelo está en disco pero es un GGML inválido (truncado por
/// una copia manual, descarga antigua interrumpida, corrupción), se borra, se
/// re-descarga limpio (con eventos de progreso) y se re-intenta una única
/// vez. Sin esto, «Reintentar» de la pantalla de error volvía a entregar el
/// mismo fichero dañado una y otra vez: bucle infinito de fallo (requisito
/// de robustez del arranque).
///
/// H3 (QA 24/09/2026): un fichero VÁLIDO que no carga (error de CUDA/memoria
/// al crear el contexto) NO se toca. Antes se borraba siempre que `preload`
/// fallara, así que con la red caída el dictado entraba en un bucle infinito
/// («Dns Failed: resolve failure», 47 veces en el log del 24/09): el usuario
/// perdía un modelo bueno y cada intento volvía a intentar descargarlo.
fn preload_with_recovery(model: Model, tx: &Sender<StartupEvent>) -> Result<(), String> {
    let first = crate::transcribe::preload(model);
    if first.is_ok() {
        return Ok(());
    }
    let err = first.unwrap_err();
    let path = crate::transcribe::model_path(model);
    if !path.is_file() {
        // Sin fichero en disco el fallo es ambiental (CUDA, memoria, …):
        // re-descargar no arregla nada, se reporta el error tal cual.
        return Err(err);
    }
    if !discard_corrupt_model(&path) {
        // El fichero es un GGML sano: el fallo es de carga (GPU/memoria) y
        // re-descargar no arregla nada. Se conserva el modelo local y se
        // propaga el fallo de carga (el usuario lo ve en la pantalla de
        // error; aquí no se borra nada ni se toca la red).
        return Err(format!(
            "{err} (el fichero del modelo en disco es válido: se conserva el modelo local)"
        ));
    }
    let redownload = crate::transcribe::ensure_model(model, |downloaded, total| {
        let _ = tx.send(StartupEvent::WhisperProgress { downloaded, total });
    });
    // Fallo al re-descargar (p. ej. sin red): mensaje clasificado y accionable
    // en español para la UI y detalle técnico en el log (dice qué se hizo).
    redownload.map_err(|error| download_failure_message(model, &error))?;
    crate::transcribe::preload(model)
}

/// ¿Hay que reciclar (borrar para re-descargar) el fichero del modelo que
/// acaba de fallar al cargar? Devuelve `true` SOLO si el fichero existe y no
/// es un GGML válido: en ese caso lo borra y lo deja dicho en el log.
///
/// Devuelve `false` —sin tocar nada— cuando no hay fichero o cuando el
/// fichero es válido: en ambos casos el fallo es ambiental (GPU, memoria…) y
/// borrar el modelo bueno solo provoca el bucle de descarga de H3.
///
/// La validez la decide `transcribe::model_file_is_valid` (única fuente de
/// verdad del formato GGML), no esta función.
fn discard_corrupt_model(path: &std::path::Path) -> bool {
    if !path.is_file() {
        return false;
    }
    if crate::transcribe::model_file_is_valid(path) {
        crate::log!(
            "startup",
            "el modelo en {} no se pudo cargar, pero el fichero es un GGML válido \
             (fallo de GPU/memoria, no del fichero): se conserva el modelo local, sin re-descargar",
            path.display()
        );
        return false;
    }
    crate::log!(
        "startup",
        "el modelo en {} no es un GGML válido (truncado o corrupto): se borra y se re-descarga",
        path.display()
    );
    let _ = std::fs::remove_file(path);
    true
}

/// Fase LLM del arranque (o del toggle de corrección IA en caliente):
/// verifica `/v1/models`; si no responde y el endpoint es local, busca y
/// lanza llama-server con el modelo configurado y espera a que quede listo
/// (máx. 60 s). Cualquier fallo → `LlmDisabled(motivo visible)` — el
/// dictado sigue funcionando sin corrección.
fn run_llm_phase(cfg: Config, tx: Sender<StartupEvent>, cancel: Arc<AtomicBool>) {
    // Punto de control 1: si el usuario apagó la corrección mientras la fase
    // estaba en vuelo (toggle off en caliente), abortar antes de tocar nada —
    // sin lanzar server, sin resolver rutas, con un único evento terminal.
    if cancel.load(Ordering::SeqCst) {
        let _ = tx.send(StartupEvent::LlmDisabled(String::new()));
        return;
    }
    let llm = cfg.llm_settings();

    if !cfg.ai_correction_enabled() {
        // El usuario la apagó: fase omitida, sin aviso (razón vacía).
        let _ = tx.send(StartupEvent::LlmDisabled(String::new()));
        return;
    }
    if !crate::llm::is_local_endpoint(&llm.base_url) {
        // Custom/Cloud remoto: no hay server local que levantar, pero SÍ
        // se comprueba que el endpoint responde (con la API key si la
        // hay; sin ella un 401 falsearía «caído»). El chip de estado
        // refleja así el proveedor activo: si el endpoint está caído o
        // rechaza la petición, la corrección se desactiva con aviso
        // visible y el dictado sigue funcionando a texto bruto.
        if crate::llm::server_alive_auth(&llm.base_url, llm.api_key.as_deref()) {
            let _ = tx.send(StartupEvent::LlmReady { launched: false });
        } else {
            let _ = tx.send(StartupEvent::LlmDisabled(remote_endpoint_reason(
                &llm.base_url,
            )));
        }
        return;
    }

    let _ = tx.send(StartupEvent::LlmStarting);
    if crate::llm::server_alive(&llm.base_url) {
        let _ = tx.send(StartupEvent::LlmReady { launched: false });
        return;
    }

    let Some(model_path) = crate::llm::resolve_model_path(&llm.model) else {
        let _ = tx.send(StartupEvent::LlmDisabled(format!(
            "no se encontró el modelo LLM «{}» en las rutas habituales. \
             La corrección IA se desactiva; el dictado sigue funcionando.",
            llm.model
        )));
        return;
    };

    let port = crate::llm::port_from_base_url(&llm.base_url);
    let Some(server_bin) = crate::llm::resolve_server_path(&cfg) else {
        // Sin ejecutable no hay server local que levantar: la corrección se
        // desactiva con aviso. El usuario puede configurar la ruta en
        // Ajustes avanzados («Server LLM») o usar un endpoint
        // OpenAI-compatible editable (proveedor Custom/Cloud).
        let _ = tx.send(StartupEvent::LlmDisabled(format!(
            "no se encontró el ejecutable del server LLM (llama-server.exe). \
             Configúralo en Ajustes o usa un endpoint OpenAI-compatible. \
             El dictado sigue funcionando."
        )));
        return;
    };
    crate::log!(
        "startup",
        "lanzando server LLM ({}): -m {} --port {}",
        server_bin.display(),
        model_path.display(),
        port
    );
    match crate::llm::launch_server(&server_bin, &model_path, port) {
        Ok(pid) => {
            // Punto de control 2: cancelación justo tras lanzar — el server
            // recién creado se termina (por su PID propio, no el slot global:
            // otra fase pudo reemplazarlo) y no se espera a que cargue el
            // modelo.
            if cancel.load(Ordering::SeqCst) {
                crate::llm::terminate_server_pid(pid);
                let _ = tx.send(StartupEvent::LlmDisabled(String::new()));
                return;
            }
            if crate::llm::wait_for_server(&llm.base_url, Duration::from_secs(60)) {
                // Punto de control 3: cancelación mientras el server cargaba
                // el modelo — si la corrección ya está apagada, el modelo no
                // debe quedarse en memoria.
                if cancel.load(Ordering::SeqCst) {
                    crate::llm::terminate_server_pid(pid);
                    let _ = tx.send(StartupEvent::LlmDisabled(String::new()));
                    return;
                }
                crate::log!("startup", "server LLM listo (lanzado por la app)");
                let _ = tx.send(StartupEvent::LlmReady { launched: true });
            } else {
                let _ = tx.send(StartupEvent::LlmDisabled(format!(
                    "el servidor LLM ({}) no respondió a /v1/models en 60 s. \
                     Mira whispr-open.log; el dictado sigue funcionando.",
                    llm.base_url
                )));
            }
        }
        Err(e) => {
            let _ = tx.send(StartupEvent::LlmDisabled(format!(
                "no se pudo lanzar el servidor LLM: {e}. El dictado sigue funcionando."
            )));
        }
    }
}

/// Razón visible cuando un endpoint remoto (proveedor Custom o Cloud)
/// no responde o rechaza la petición. El dictado NUNCA se bloquea: la
/// corrección se desactiva con este aviso y el turno se inserta a texto
/// bruto (spec §3.5.4).
fn remote_endpoint_reason(base_url: &str) -> String {
    format!(
        "el endpoint {base_url} no respondió o rechazó la petición; \
         el dictado sigue funcionando sin corrección"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Transparencia del corrector IA (requisito UX 10/09/2026): el chip del
    /// estado debe decir SIEMPRE algo coherente con (toggle, estado del
    /// server) y prometer el modelo solo cuando la corrección puede actuar.
    #[test]
    fn chip_muestra_modelo_solo_cuando_la_correccion_esta_activa() {
        let ui = llm_status_ui(true, &LlmStatus::Ready { launched: false }, "ornith-1.0-9b");
        assert_eq!(ui.kind, "ok");
        assert!(ui.label.contains("activa"));
        assert!(ui.label.contains("ornith-1.0-9b"));
    }

    #[test]
    fn chip_dice_apagada_con_toggle_off_y_no_promete_modelo() {
        let ui = llm_status_ui(false, &LlmStatus::Ready { launched: true }, "gpt-4o-mini");
        assert_eq!(ui.kind, "dim");
        assert!(ui.label.contains("apagada"));
        assert!(!ui.label.contains("gpt-4o-mini"));
    }

    #[test]
    fn chip_no_esconde_el_motivo_de_estar_inactiva() {
        let ui =
            llm_status_ui(true, &LlmStatus::Disabled("servidor no encontrado".into()), "");
        assert_eq!(ui.kind, "warn");
        assert!(ui.label.contains("servidor no encontrado"));
    }

    #[test]
    fn chip_avisa_mientras_comprueba_el_server() {
        let ui = llm_status_ui(true, &LlmStatus::Unknown, "");
        assert_eq!(ui.kind, "dim");
        assert!(ui.label.contains("comprobando"));
    }

    /// Ciclo de vida 10/09/2026: una fase LLM cancelada en caliente (toggle
    /// off mientras el server se lanza/carga) aborta SIN tocar nada — emite
    /// un único `LlmDisabled` con razón vacía (desactivada por el usuario,
    /// sin banner de error). Con el token ya puesto, la fase nunca llega a
    /// resolver rutas ni a lanzar llama-server.
    #[test]
    fn fase_llm_cancelada_no_lanza_server() {
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let (tx, rx) = std::sync::mpsc::channel();
        run_llm_phase(Config::default(), tx, cancel);
        match rx.try_recv() {
            Ok(StartupEvent::LlmDisabled(reason)) => assert!(
                reason.is_empty(),
                "cancelación por usuario no debe mostrar banner, llegó {reason:?}"
            ),
            other => panic!("esperaba LlmDisabled(razón vacía), llegó {other:?}"),
        }
        assert!(
            rx.try_recv().is_err(),
            "la fase cancelada emite un único evento terminal"
        );
    }

    /// Proveedor Custom/Cloud remoto caído (requisito 10/09/2026): la
    /// razón visible del chip nombra el endpoint y deja claro que el
    /// dictado NUNCA se bloquea (degrada a texto bruto).
    #[test]
    fn endpoint_remoto_caido_desactiva_con_aviso_que_no_bloquea() {
        let reason = remote_endpoint_reason("https://api.ejemplo.com/v1");
        assert!(reason.contains("api.ejemplo.com"), "nombra el endpoint");
        assert!(
            reason.contains("el dictado sigue funcionando sin corrección"),
            "deja claro que no bloquea: {reason}"
        );
        assert!(reason.contains("no respondió o rechazó"));
    }

    /// Robustez: los avisos de la línea de feedback (micro desconectado,
    /// turno vacío, tope de grabación) se pintan ámbar, no verde — un
    /// «todo bien» verde sobre un micro muerto es mentira visual.
    #[test]
    fn feedback_de_robustez_se_clasifica_como_aviso() {
        assert!(feedback_is_warning(
            "El micrófono se desconectó a mitad: se transcribió lo que se oyó hasta ese momento."
        ));
        assert!(feedback_is_warning("No se oyó nada: comprueba el micrófono."));
        assert!(feedback_is_warning(
            "Dictado larguísimo: parado a las 2 h para no agotar la memoria."
        ));
        assert!(feedback_is_warning("Esperando el resto de la corrección…"));
        // H6: el aviso de «micro configurado ausente, se usa el por defecto»
        // también es un aviso ámbar, no una confirmación verde.
        assert!(feedback_is_warning(&crate::audio::fallback_notice(
            "Línea (4- AudioBox USB)",
            "Micrófono (Realtek(R) Audio)"
        )));
        // Confirmaciones: verde.
        assert!(!feedback_is_warning("Texto escrito en la app activa ✓"));
        assert!(!feedback_is_warning("Corrección IA aplicada: «4» → «2»"));
    }

    /// Robustez: el error de descarga de modelo que llega a `Status::Error`
    /// lleva causa clasificada y accionable, no la cadena cruda de ureq.
    #[test]
    fn mensaje_de_fallo_de_descarga_es_accionable_y_sin_jerga() {
        let msg = download_failure_message(
            Model::Small,
            "no se pudo descargar el modelo: ...: Dns Failed: resolve failure",
        );
        assert!(msg.contains("Small"), "nombra el modelo: {msg}");
        assert!(msg.contains("sin conexión"), "clasifica la causa: {msg}");
        assert!(
            !msg.contains("Dns Failed"),
            "sin jerga de la cadena de crates: {msg}"
        );
        assert!(msg.contains("Revisa"), "accionable: {msg}");
    }

    /// Robustez del arranque: `preload_with_recovery` con un modelo SIN
    /// fichero en disco devuelve el error de whisper tal cual (el fallo es
    /// ambiental, no del fichero: re-descargar no haría nada) y no toca la
    /// red. Solo se ejecuta si el usuario no tiene el modelo small
    /// descargado (si lo tiene, cargarlo de verdad es lento y no es cosa
    /// de un test). La ruta de recuperación completa (fichero corrupto →
    /// borrar → re-descargar) se prueba por piezas: `model_file_is_valid`,
    /// `existing_model_plan` y `sweep_part_files` en transcribe.rs.
    #[test]
    fn preload_sin_fichero_falla_claro_y_sin_red() {
        if crate::transcribe::model_exists(Model::Small) {
            return; // el modelo real del usuario no se toca en un test
        }
        let (tx, _rx) = std::sync::mpsc::channel();
        let result = preload_with_recovery(Model::Small, &tx);
        assert!(
            result.is_err(),
            "sin fichero en disco, cargar el contexto debe fallar con error"
        );
    }

    /// H3 (QA 24/09/2026): con el modelo YA en disco, un fallo de carga NO
    /// puede borrarlo. Antes `preload_with_recovery` borraba el fichero ante
    /// cualquier fallo de `preload` y re-descargaba: con la red caída eso es
    /// un bucle infinito («Dns Failed: resolve failure», 47 veces en el log
    /// del 24/09) que además destruía el modelo bueno del usuario. Un fichero
    /// truncado/corrupto SÍ se recicla (única re-descarga legítima).
    #[test]
    fn h3_modelo_valido_se_conserva_y_el_truncado_se_recicla() {
        let dir = std::env::temp_dir().join(format!("whispr-h3-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir temporal del test");

        // GGML válido de mentira: magic de whisper.cpp y tamaño sobre el
        // mínimo que exige `model_file_is_valid`.
        let valido = dir.join("ggml-valido.bin");
        let mut bytes = vec![0x6c, 0x6d, 0x67, 0x67];
        bytes.resize(1024 * 1024 + 16, 0);
        std::fs::write(&valido, &bytes).expect("escribir modelo válido");
        assert!(crate::transcribe::model_file_is_valid(&valido));

        assert!(
            !discard_corrupt_model(&valido),
            "un GGML válido no se recicla: el fallo fue de GPU/memoria, no del fichero"
        );
        assert!(
            valido.is_file(),
            "el modelo local se conserva (sin re-descarga, sin tocar la red)"
        );

        // Truncado (magic correcto pero tamaño por debajo del mínimo): se
        // recicla para que la descarga lo reponga.
        let truncado = dir.join("ggml-truncado.bin");
        std::fs::write(&truncado, [0x6c, 0x6d, 0x67, 0x67, 0x00]).expect("escribir truncado");
        assert!(!crate::transcribe::model_file_is_valid(&truncado));
        assert!(
            discard_corrupt_model(&truncado),
            "un fichero truncado sí se recicla"
        );
        assert!(
            !truncado.exists(),
            "el fichero inválido se borra para poder re-descargarlo"
        );

        // Sin fichero no hay nada que reciclar: el fallo es ambiental.
        assert!(!discard_corrupt_model(&dir.join("no-hay.bin")));

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── Nota editable (requisito R3) ────────────────────────────────────

    /// Tras una edición manual, «lo último» es la última línea con
    /// contenido: la corrección cross-turn y «Escribir en app» siguen
    /// apuntando a lo que el usuario acaba de ver/tocar.
    #[test]
    fn last_turn_tras_edicion_manual_es_la_ultima_linea() {
        assert_eq!(last_turn_from_transcript("hola mundo"), "hola mundo");
        assert_eq!(
            last_turn_from_transcript("primera línea\nsegunda línea"),
            "segunda línea"
        );
        // Líneas vacías al final (saltos del usuario) no cuentan.
        assert_eq!(
            last_turn_from_transcript("turno bueno\n\n   \n"),
            "turno bueno"
        );
        assert_eq!(last_turn_from_transcript(""), "");
        assert_eq!(last_turn_from_transcript("\n \n"), "");
    }

    /// El editor sincronizado con Content::text() recorta el '\n' final de
    /// cortesía de la API: sin esto, cada edición acumularía saltos falsos.
    #[test]
    fn la_edicion_manual_no_acumula_saltos_falsos() {
        // El propio contrato de Content::text() añade '\n' al final: el
        // handler de TranscriptEdited recorta ANTES de asignar transcript.
        let content =
            text_editor::Content::<iced::Renderer>::with_text("una línea");
        let raw = content.text();
        assert!(raw.ends_with('\n'), "la API añade el salto de cortesía");
        assert_eq!(raw.trim_end_matches('\n'), "una línea");
    }

    // ── Descarga con MB y tiempo restante (requisito R4) ────────────────

    #[test]
    fn etiqueta_de_descarga_lleva_mb_y_eta() {
        let total = 466 * 1024 * 1024u64;
        let mitad = total / 2;
        // Sin velocidad aún: calculando.
        let sin_rate = download_progress_label("Small", mitad, total, None);
        assert!(sin_rate.contains("233 / 466 MB"), "{sin_rate}");
        assert!(sin_rate.contains("calculando tiempo"), "{sin_rate}");
        // Con velocidad de ~10 MB/s: ~23 s restantes.
        let con_rate = download_progress_label(
            "Small",
            mitad,
            total,
            Some(10.0 * 1024.0 * 1024.0),
        );
        assert!(con_rate.contains("quedan ~"), "{con_rate}");
        assert!(con_rate.contains("23 s"), "{con_rate}");
        // Total desconocido: sin inventar MB.
        let desconocido = download_progress_label("Tiny", 5, 0, None);
        assert!(desconocido.contains("tamaño desconocido"), "{desconocido}");
    }

    #[test]
    fn eta_no_promete_exactitud_donde_no_la_hay() {
        let total = 466 * 1024 * 1024u64;
        let d = 0;
        // Velocidad ridícula (~60 KB/s → ~2 h 15 min) → «más de 1 h», sin
        // minutos falsos.
        assert_eq!(
            download_eta_text(d, total, Some(60_000.0)),
            "quedan más de 1 h"
        );
        // Velocidad incapaz de medirse → calculando.
        assert_eq!(download_eta_text(d, total, Some(500.0)), "calculando tiempo…");
        assert_eq!(download_eta_text(d, total, None), "calculando tiempo…");
        // Minutos normales redondeados.
        let rate = total as f64 / 180.0; // 3 min exactos
        assert_eq!(download_eta_text(0, total, Some(rate)), "quedan ~3 min");
        // Ya descargado todo: casi listo.
        assert_eq!(download_eta_text(total, total, Some(rate)), "casi listo");
    }

    /// El pie de confianza es honesto (R4): con la corrección en la nube
    /// activa NO promete «nada sale de tu PC».
    #[test]
    fn pie_de_confianza_no_miente_con_la_nube_activa() {
        assert_eq!(
            trust_footer_line(false),
            "Transcripción local · nada sale de tu PC"
        );
        let con_nube = trust_footer_line(true);
        assert!(con_nube.contains("nube"), "{con_nube}");
        assert!(!con_nube.contains("nada sale"), "{con_nube}");
    }

    /// Los nombres del selector de proveedor (requisito 10/09/2026) son
    /// autoexplicativos: Local = llama.cpp que viene con la app (default),
    /// Custom = endpoint OpenAI-compatible, Cloud = OpenAI.
    #[test]
    fn etiquetas_de_proveedor_dicen_exactamente_lo_que_hacen() {
        let local = CorrectionProvider::Default.label();
        assert!(local.contains("Local"));
        assert!(local.contains("llama.cpp"));
        assert!(local.contains("viene con la app"));

        let custom = CorrectionProvider::Custom.label();
        assert!(custom.contains("Custom LLM"));
        assert!(custom.contains("OpenAI-compatible"));

        let cloud = CorrectionProvider::Cloud.label();
        assert!(cloud.contains("Cloud"));
        assert!(cloud.contains("OpenAI"));
    }
    // ── UI 24/09/2026: secciones de la ventana y aviso del medidor ──────────

    #[test]
    fn la_ventana_tiene_dos_secciones_con_etiqueta_visible() {
        // Una pantalla, una pregunta: no se inventan pestañas de más.
        assert_eq!(WindowSection::ALL.len(), 2);
        assert_eq!(WindowSection::ALL[0], WindowSection::Dictado);
        assert_eq!(WindowSection::ALL[1], WindowSection::Ajustes);
        assert_eq!(WindowSection::Dictado.label(), "Dictado");
        assert_eq!(WindowSection::Ajustes.label(), "Ajustes");
        // La sección se identifica por texto (accesible sin depender del color).
        assert!(WindowSection::Dictado.to_string().contains("Dictado"));
    }

    #[test]
    fn el_aviso_del_medidor_solo_sale_cuando_aporta() {
        assert_eq!(mic_peak_hint(50, 30), None, "nivel normal: sin aviso");
        assert_eq!(mic_peak_hint(MIC_SILENT_PERCENT, 30), None);
        assert_eq!(mic_peak_hint(MIC_CLIP_PERCENT - 1, 30), None);
        // Saturado: avisa desde el primer instante del turno.
        assert_eq!(
            mic_peak_hint(MIC_CLIP_PERCENT, 0),
            Some(MicPeakHint::Clipping)
        );
        assert_eq!(mic_peak_hint(100, 0), Some(MicPeakHint::Clipping));
        // Mudo: solo pasado el margen inicial (no parpadea al empezar a grabar).
        assert_eq!(mic_peak_hint(0, 0), None);
        assert_eq!(mic_peak_hint(0, MIC_SILENT_SECS - 1), None);
        assert_eq!(
            mic_peak_hint(0, MIC_SILENT_SECS),
            Some(MicPeakHint::Silent)
        );
        // Los avisos se leen: texto accionable, no solo color.
        assert!(MicPeakHint::Silent.label().contains("sin señal"));
        assert!(MicPeakHint::Clipping.label().contains("satur"));
    }
}
