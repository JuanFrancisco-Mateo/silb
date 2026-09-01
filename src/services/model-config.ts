export type WhisperModelId = 'tiny' | 'base' | 'small'

export type ModelOption = {
  id: WhisperModelId
  label: string
}

export const MODEL_STORAGE_KEY = 'whispr-open-model'

export const MODEL_OPTIONS: readonly ModelOption[] = [
  { id: 'tiny', label: 'Tiny (rápido)' },
  { id: 'base', label: 'Base (equilibrado)' },
  { id: 'small', label: 'Small (preciso)' },
]

const DEFAULT_MODEL: WhisperModelId = 'tiny'

export function isModelId(value: unknown): value is WhisperModelId {
  return MODEL_OPTIONS.some((option) => option.id === value)
}

/** Lee el modelo guardado; cualquier valor ausente o desconocido cae en 'tiny'. */
export function getModel(): WhisperModelId {
  try {
    const raw = localStorage.getItem(MODEL_STORAGE_KEY)
    if (raw && isModelId(raw)) return raw
  } catch {
    // localStorage no disponible (tests en node, modo privado, etc.)
  }
  return DEFAULT_MODEL
}

/** Persiste el modelo elegido. Devuelve false si el id es inválido o no se pudo guardar. */
export function setModel(id: WhisperModelId): boolean {
  if (!isModelId(id)) return false
  try {
    localStorage.setItem(MODEL_STORAGE_KEY, id)
    return true
  } catch {
    return false
  }
}