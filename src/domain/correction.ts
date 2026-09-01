export type Correction = {
  id: string
  original: string
  replacement: string
  reason: string
  createdAt: string
}

export type CorrectionResult = {
  text: string
  correction: Correction | null
}

export type CorrectionTurn = {
  text: string
  correction: Correction | null
  pending: boolean
}

const correctionPatterns = [
  {
    pattern: /^(?:uy[,;:.!? ]*)?(?:así|asi) no (?:era|es)[,;:.!? ]*(?:era|quise decir)[,;:.!? ]+(.+)$/i,
    reason: 'Rectificación detectada en tu frase',
  },
  {
    pattern: /^(?:no[,;:.!? ]*)?(?:así|asi) no[,;:.!? ]*(?:dije|era)[,;:.!? ]*(?:lo que quería decir era|quise decir)[,;:.!? ]+(.+)$/i,
    reason: 'Rectificación detectada en tu frase',
  },
  {
    pattern: /^no era .+[,;:.!?]+[ ]*(?:era|es)[,;:.!? ]+(.+)$/i,
    reason: 'Rectificación detectada',
  },
  {
    pattern: /^no[,;:.!? ]+(?:era|es)[,;:.!? ]+(.+)$/i,
    reason: 'Rectificación detectada',
  },
  {
    pattern: /^donde dije .+[,;:.!? ]+(?:quer[ií]a|quise) decir[,;:.!? ]+(.+)$/i,
    reason: 'Reformulación detectada',
  },
  {
    pattern: /^(?:no[,;:.!? ]+)?(?:quer[ií]a|quise) decir[,;:.!? ]+(?:era|es)[,;:.!? ]+(.+)$/i,
    reason: 'Rectificación detectada',
  },
  {
    pattern: /^(?:no[,;:.!? ]+)?(?:quer[ií]a|quise) decir .+[,;:.!? ]+(?:quer[ií]a|quise) decir[,;:.!? ]+(.+)$/i,
    reason: 'Rectificación detectada',
  },
  {
    pattern: /^(?:no[,;:.!? ]+)?(?:quer[ií]a|quise) decir(?!\s+que\b)[,;:.!? ]+(.+)$/i,
    reason: 'Rectificación detectada',
  },
  {
    pattern: /^perd[oó]n(?:a|eme)?[,;:.!? ]+(?:(?:quer[ií]a|quise) decir|era|es)[,;:.!? ]+(.+)$/i,
    reason: 'Rectificación después de una disculpa',
  },
  {
    pattern: /^(?:bueno[,;:.!? ]+)?mejor dicho[,;:.!? ]+(.+)$/i,
    reason: 'Reformulación detectada',
  },
  {
    pattern: /^(?:bueno[,;:.!? ]+)?m[aá]s bien[,;:.!? ]+(.+)$/i,
    reason: 'Reformulación detectada',
  },
  {
    pattern: /^me refer[ií]a ((?:a la|al)\s+.+)$/i,
    reason: 'Aclaración detectada',
  },
  {
    pattern: /^me refer[ií]a a[,;:.!? ]+(.+)$/i,
    reason: 'Aclaración detectada',
  },
  {
    pattern: /^(?:la palabra correcta|lo correcto)[,;:.!? ]+(?:es|ser[ií]a)[,;:.!? ]+(.+)$/i,
    reason: 'Corrección explícita detectada',
  },
  {
    pattern: /^lo que quería decir era[,;:.!? ]+(.+)$/i,
    reason: 'Rectificación detectada',
  },
  {
    pattern: /^me equivoqu[eé][,;:.!? ]+(?:(?:quer[ií]a|quise) decir|era|es)[,;:.!? ]+(.+)$/i,
    reason: 'Rectificación después de reconocer un error',
  },
  {
    pattern: /^(?:rectifico|corrijo)[,;:.!? ]+(.+)$/i,
    reason: 'Corrección explícita detectada',
  },
  {
    pattern: /^(?:digo|o mejor)[,;:.!?]+[ ]*(.+)$/i,
    reason: 'Reformulación detectada',
  },
  {
    pattern: /^.+[,;] *no[,;:.!? ]+(.+)$/i,
    reason: 'Autocorrección detectada',
  },
]

const normalizeSpaces = (value: string) => value.replace(/\s+/g, ' ').trim()

const capitalizeSentence = (value: string) => {
  const clean = normalizeSpaces(value)
  if (!clean) return clean
  return clean.charAt(0).toUpperCase() + clean.slice(1)
}

function isCorrectionStart(value: string) {
  return /^(?:uy[,;:.!? ]*)?(?:así|asi) no (?:era|es)\.?$/i.test(value)
    || /^(?:no[,;:.!? ]*)?(?:así|asi) no[,;:.!? ]*(?:dije|era)\.?$/i.test(value)
    || /^(?:no[,;:.!? ]*)?(?:así|asi) no[,;:.!? ]*(?:dije|era)[,;:.!? ]*(?:lo que quería decir era|quise decir)\.?$/i.test(value)
    || /^(?:no[,;:.!? ]+)?(?:quer[ií]a|quise) decir[,;:.!? ]*$/i.test(value)
    || /^perd[oó]n(?:a|eme)?[,;:.!? ]+(?:(?:quer[ií]a|quise) decir|era|es)[,;:.!? ]*$/i.test(value)
    || /^(?:bueno[,;:.!? ]+)?mejor dicho[,;:.!? ]*$/i.test(value)
    || /^(?:bueno[,;:.!? ]+)?m[aá]s bien[,;:.!? ]*$/i.test(value)
    || /^me refer[ií]a (?:a|al|a la)[,;:.!? ]*$/i.test(value)
    || /^(?:la palabra correcta|lo correcto)[,;:.!? ]+(?:es|ser[ií]a)[,;:.!? ]*$/i.test(value)
    || /^lo que quería decir era[,;:.!? ]*$/i.test(value)
    || /^donde dije .+[,;:.!? ]*$/i.test(value)
    || /^no era .+[,;:.!? ]*$/i.test(value)
    || /^digo$/i.test(value)
    || /^o mejor$/i.test(value)
    || /^.+[,;] *no[,;:.!? ]*$/i.test(value)
}

export function applyCorrection(input: string): CorrectionResult {
  const value = normalizeSpaces(input)

  for (const { pattern, reason } of correctionPatterns) {
    const match = value.match(pattern)
    if (!match?.[1]) continue

    const replacement = capitalizeSentence(match[1])
    return {
      text: replacement,
      correction: {
        id: crypto.randomUUID(),
        original: value,
        replacement,
        reason,
        createdAt: new Date().toLocaleTimeString('es-ES', { hour: '2-digit', minute: '2-digit' }),
      },
    }
  }

  return { text: value, correction: null }
}

export function appendTranscript(current: string, incoming: string) {
  const next = normalizeSpaces(incoming)
  if (!next) return current
  return normalizeSpaces(`${current} ${next}`)
}

export function replaceLastSentence(current: string, replacement: string) {
  const next = capitalizeSentence(replacement)
  if (!next) return current

  const lastBoundary = Math.max(current.lastIndexOf('.'), current.lastIndexOf('!'), current.lastIndexOf('?'))
  if (lastBoundary === -1) return next

  return `${current.slice(0, lastBoundary + 1)} ${next}`.trim()
}

export class CorrectionSession {
  private pendingTurn = ''

  accept(turn: string): CorrectionTurn {
    const value = normalizeSpaces(turn)
    if (!value) return { text: '', correction: null, pending: Boolean(this.pendingTurn) }

    if (this.pendingTurn) {
      const combined = appendTranscript(this.pendingTurn, value)
      const candidate = `${this.pendingTurn.replace(/[,;:.!?]+$/, '')}, ${value}`
      const correction = applyCorrection(candidate)
      if (correction.correction) {
        this.pendingTurn = ''
        correction.correction.original = candidate
        return { text: correction.text, correction: correction.correction, pending: false }
      }

      if (isCorrectionStart(candidate)) {
        this.pendingTurn = candidate
        return { text: '', correction: null, pending: true }
      }

      const flushed = combined
      this.pendingTurn = ''
      return { text: flushed, correction: null, pending: false }
    }

    const correction = applyCorrection(value)
    if (correction.correction) return { text: correction.text, correction: correction.correction, pending: false }

    if (isCorrectionStart(value)) {
      this.pendingTurn = value
      return { text: '', correction: null, pending: true }
    }

    return { text: value, correction: null, pending: false }
  }

  reset() {
    this.pendingTurn = ''
  }

  hasPendingCorrection() {
    return Boolean(this.pendingTurn)
  }
}
