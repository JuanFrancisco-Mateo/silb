export type DictionaryEntry = {
  from: string
  to: string
}

export type DictionaryResult = {
  text: string
  applied: string[]
}

export const DICTIONARY_STORAGE_KEY = 'whispr-open-dictionary'

const escapeRegExp = (value: string) => value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')

/**
 * Patrón de palabra completa, case-insensitive y con límites Unicode:
 * el término no puede estar rodeado de letras, números ni marcas de
 * combinación (sí puede estar rodeado de espacios, puntuación o límites del texto).
 */
const toWordPattern = (value: string) =>
  new RegExp(`(?<![\\p{L}\\p{M}\\p{N}])${escapeRegExp(value)}(?![\\p{L}\\p{M}\\p{N}])`, 'giu')

function isValidEntry(value: unknown): value is DictionaryEntry {
  return (
    typeof value === 'object' &&
    value !== null &&
    typeof (value as DictionaryEntry).from === 'string' &&
    typeof (value as DictionaryEntry).to === 'string'
  )
}

/**
 * Aplica las entradas del diccionario sobre el texto, en orden.
 * Cada término se reemplaza con coincidencia de palabra completa y
 * case-insensitive, preservando la capitalización de `to`.
 * Devuelve el texto resultante y la lista (única) de términos aplicados.
 */
export function applyDictionary(text: string, entries: DictionaryEntry[]): DictionaryResult {
  if (!text) return { text: '', applied: [] }

  let result = text
  const applied: string[] = []

  for (const entry of entries) {
    const from = entry.from.trim()
    if (!from) continue

    const pattern = toWordPattern(from)
    if (!result.match(pattern)) continue

    result = result.replace(pattern, entry.to)
    if (!applied.includes(from)) applied.push(from)
  }

  return { text: result, applied }
}

/** Guarda el diccionario en localStorage. Devuelve false si no se pudo persistir. */
export function saveDictionary(entries: DictionaryEntry[]): boolean {
  try {
    localStorage.setItem(DICTIONARY_STORAGE_KEY, JSON.stringify(entries))
    return true
  } catch {
    return false
  }
}

/** Lee el diccionario desde localStorage. Devuelve [] si no hay nada o hay error. */
export function loadDictionary(): DictionaryEntry[] {
  try {
    const raw = localStorage.getItem(DICTIONARY_STORAGE_KEY)
    if (!raw) return []
    const parsed: unknown = JSON.parse(raw)
    if (!Array.isArray(parsed)) return []
    return parsed.filter(isValidEntry).map(({ from, to }) => ({ from, to }))
  } catch {
    return []
  }
}