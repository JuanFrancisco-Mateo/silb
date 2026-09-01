export type CleanupResult = {
  text: string
  removed: string[]
}

type CleanupRule = {
  label: string
  pattern: RegExp
}

// Cada patrón captura dos grupos:
//   grupo 1 -> texto que se CONSERVA (la frontera anterior a la muletilla,
//              o la primera ocurrencia en una repetición)
//   grupo 2 -> texto que se ELIMINA y se reporta en `removed`
const cleanupRules: CleanupRule[] = [
  // Muletillas inequívocas (ES/EN): cualquier posición, palabra completa
  { label: 'eh', pattern: /(^|[\s(«"'¡¿…])(eh)([,;:…]*)(?=$|[\s,;:.!?…])/giu },
  { label: 'um', pattern: /(^|[\s(«"'¡¿…])(um)([,;:…]*)(?=$|[\s,;:.!?…])/giu },
  { label: 'uh', pattern: /(^|[\s(«"'¡¿…])(uh)([,;:…]*)(?=$|[\s,;:.!?…])/giu },
  { label: 'o sea', pattern: /(^|[\s(«"'¡¿…])(o sea)([,;:…]*)(?=$|[\s,;:.!?…])/giu },
  { label: 'you know', pattern: /(^|[\s(«"'¡¿…])(you know)([,;:…]*)(?=$|[\s,;:.!?…])/giu },

  // Muletillas ambiguas: solo cuando van seguidas de una pausa
  // (coma, punto y coma, dos puntos o puntos suspensivos), para no romper
  // usos legítimos ("este es el plan", "un tipo de documento", "I like pizza").
  { label: 'este', pattern: /(^|[\s(«"'¡¿…])(este)([,;:…]+)/giu },
  { label: 'esto', pattern: /(^|[\s(«"'¡¿…])(esto)([,;:…]+)/giu },
  { label: 'tipo', pattern: /(^|[\s(«"'¡¿…])(tipo)([,;:…]+)/giu },
  { label: 'like', pattern: /(^|[\s(«"'¡¿…])(like)([,;:…]+)/giu },

  // Muletillas solo al inicio del turno de dictado
  { label: 'bueno', pattern: /(^)(bueno)([,;:.!?…]*)/giu },
  { label: 'pues (inicio)', pattern: /(^)(pues)([,;:.!?…]*)/giu },
  { label: 'pues (pausa)', pattern: /([\s])(pues)([,;:…]+)/giu },

  // Repeticiones de palabra y de bigrama -> una sola ocurrencia
  { label: 'repetición de palabra', pattern: /\b(\p{L}+)(\s+\1)\b/giu },
  { label: 'repetición de bigrama', pattern: /\b((?:\p{L}+\s+)\p{L}+)(\s+\1)\b/giu },
]

const normalizeSpaces = (value: string) => value.replace(/\s+/g, ' ').trim()

const normalizePunctuation = (value: string) =>
  value
    .replace(/\s+([,;:.!?…])/g, '$1')
    .replace(/[,;:]([.!?…])/g, '$1')
    .replace(/([«"'¿¡(…])\s+/g, '$1')

// Un turno que solo contiene pausas (o puntuación sin texto) no es texto que
// limpiar: se devuelve intacto para no destruir respuestas como "Bueno.".
const isFillerOnly = (value: string) =>
  value === '' || /^[\s,;:.!?…()«"»'¡¿\-]+$/.test(value)

export function cleanTranscript(input: string): CleanupResult {
  const original = normalizeSpaces(input)
  if (!original) return { text: '', removed: [] }

  let text = original
  const removed: string[] = []
  let removedAtStart = false

  for (const rule of cleanupRules) {
    let iterations = 0
    while (iterations++ < 10) {
      const before = text
      // Los patrones tienen 2 o 3 grupos de captura, así que el offset real
      // es siempre el penúltimo argumento del callback.
      text = text.replace(rule.pattern, (...args: Array<string | number>) => {
        const keep = args[1] as string
        const piece = args[2] as string
        const offset = args[args.length - 2] as number
        if (offset === 0) removedAtStart = true
        removed.push(piece.trim())
        return keep
      })
      if (text === before) break
      text = normalizeSpaces(text)
    }
  }

  text = normalizePunctuation(text)
  text = normalizeSpaces(text)

  // Al eliminar una muletilla (o repetición) que abría el turno, la primera
  // palabra pasa a encabezar la frase: se capitaliza sin tocar el resto.
  if (removedAtStart) {
    text = text.replace(/^([^\p{L}]*)(\p{Ll})/u, (_, prefix: string, firstLetter: string) => prefix + firstLetter.toUpperCase())
  }

  if (isFillerOnly(text)) return { text: original, removed: [] }

  return { text, removed: [...new Set(removed)] }
}