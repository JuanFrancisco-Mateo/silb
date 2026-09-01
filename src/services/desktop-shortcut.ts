const GLOBAL_DICTATION_EVENT = 'global-dictation-toggle'

type Unlisten = () => void

export async function listenForGlobalDictation(onToggle: () => void): Promise<Unlisten> {
  if (!window.__TAURI_INTERNALS__) return () => undefined

  const { listen } = await import('@tauri-apps/api/event')
  return listen(GLOBAL_DICTATION_EVENT, () => onToggle())
}
