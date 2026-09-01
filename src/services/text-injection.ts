const TYPE_TEXT_COMMAND = 'type_text'

export interface InjectionResult {
  ok: boolean
  error?: string
}

interface TauriInternals {
  invoke: (command: string, args?: Record<string, unknown>) => Promise<unknown>
}

function getTauriInvoke(): TauriInternals['invoke'] | null {
  const internals =
    typeof window !== 'undefined'
      ? (window.__TAURI_INTERNALS__ as TauriInternals | undefined)
      : undefined
  return typeof internals?.invoke === 'function' ? internals.invoke.bind(internals) : null
}

/**
 * Escribe `text` en la aplicación activa a través del comando nativo
 * `type_text` (portapapeles + Ctrl+V en Windows). En el navegador (sin el
 * runtime Tauri) devuelve `{ ok: false, error: 'web' }` de forma silenciosa.
 */
export async function injectTextIntoActiveApp(text: string): Promise<InjectionResult> {
  const invoke = getTauriInvoke()
  if (!invoke) return { ok: false, error: 'web' }

  try {
    await invoke(TYPE_TEXT_COMMAND, { text })
    return { ok: true }
  } catch (error) {
    return {
      ok: false,
      error: error instanceof Error ? error.message : String(error),
    }
  }
}