import { describe, expect, it } from 'vitest'
import { CorrectionSession, applyCorrection } from './correction'

const withoutVolatileFields = (value: ReturnType<typeof applyCorrection>) => ({
  text: value.text,
  hasCorrection: Boolean(value.correction),
  original: value.correction?.original,
  replacement: value.correction?.replacement,
  reason: value.correction?.reason,
})

describe('applyCorrection', () => {
  it('applies a complete correction in one turn', () => {
    expect(withoutVolatileFields(applyCorrection('Uy, así no era, era el informe del jueves'))).toEqual({
      text: 'El informe del jueves',
      hasCorrection: true,
      original: 'Uy, así no era, era el informe del jueves',
      replacement: 'El informe del jueves',
      reason: 'Rectificación detectada en tu frase',
    })
  })

  it.each([
    ['No, quería decir el jueves', 'El jueves', 'Rectificación detectada'],
    ['No, era el jueves', 'El jueves', 'Rectificación detectada'],
    ['Perdón, era la reunión del viernes', 'La reunión del viernes', 'Rectificación después de una disculpa'],
    ['Perdona, quería decir la reunión del viernes', 'La reunión del viernes', 'Rectificación después de una disculpa'],
    ['Mejor dicho, el informe final', 'El informe final', 'Reformulación detectada'],
    ['Más bien, el informe final', 'El informe final', 'Reformulación detectada'],
    ['Me refería al informe final', 'Al informe final', 'Aclaración detectada'],
    ['La palabra correcta es presupuesto', 'Presupuesto', 'Corrección explícita detectada'],
    ['Lo correcto sería el jueves', 'El jueves', 'Corrección explícita detectada'],
    ['Lo que quería decir era el jueves', 'El jueves', 'Rectificación detectada'],
    ['Me equivoqué, quería decir el jueves', 'El jueves', 'Rectificación después de reconocer un error'],
    ['Rectifico: el jueves', 'El jueves', 'Corrección explícita detectada'],
    ['Corrijo, la reunión del viernes', 'La reunión del viernes', 'Corrección explícita detectada'],
    ['Digo, el jueves', 'El jueves', 'Reformulación detectada'],
    ['O mejor, el jueves', 'El jueves', 'Reformulación detectada'],
    ['El martes, no, el jueves', 'El jueves', 'Autocorrección detectada'],
    ['Donde dije martes, quería decir jueves', 'Jueves', 'Reformulación detectada'],
    ['No era martes, era jueves', 'Jueves', 'Rectificación detectada'],
  ])('detects "%s" as a correction', (input, replacement, reason) => {
    expect(withoutVolatileFields(applyCorrection(input))).toEqual({
      text: replacement,
      hasCorrection: true,
      original: input,
      replacement,
      reason,
    })
  })

  it('accepts punctuation between the correction marker and replacement', () => {
    expect(withoutVolatileFields(applyCorrection('Así no era. Era la reunión del viernes'))).toEqual({
      text: 'La reunión del viernes',
      hasCorrection: true,
      original: 'Así no era. Era la reunión del viernes',
      replacement: 'La reunión del viernes',
      reason: 'Rectificación detectada en tu frase',
    })
  })

  it.each([
    'Preparar el informe para el jueves',
    'La reunión es el jueves',
    'Digo que la reunión empieza a las diez',
    'Quise decir que el informe ya está listo',
  ])('leaves ordinary speech unchanged: "%s"', (input) => {
    expect(withoutVolatileFields(applyCorrection(input))).toEqual({
      text: input,
      hasCorrection: false,
      original: undefined,
      replacement: undefined,
      reason: undefined,
    })
  })
})

describe('CorrectionSession', () => {
  it('holds a correction prefix until the next turn completes it', () => {
    const session = new CorrectionSession()

    expect(session.accept('Uy, así no era')).toEqual({ text: '', correction: null, pending: true })
    expect(session.hasPendingCorrection()).toBe(true)

    const completed = session.accept('era el informe del jueves')
    expect(completed.text).toBe('El informe del jueves')
    expect(completed.correction?.original).toBe('Uy, así no era, era el informe del jueves')
    expect(completed.correction?.replacement).toBe('El informe del jueves')
    expect(completed.pending).toBe(false)
    expect(session.hasPendingCorrection()).toBe(false)
  })

  it.each([
    ['No, quería decir', 'el jueves', 'El jueves'],
    ['Perdón, quería decir', 'la reunión del viernes', 'La reunión del viernes'],
    ['Mejor dicho', 'el informe final', 'El informe final'],
    ['Me refería a', 'la reunión del viernes', 'La reunión del viernes'],
    ['Digo', 'el jueves', 'El jueves'],
  ])('completes "%s" split across two turns', (prefix, replacement, expected) => {
    const session = new CorrectionSession()

    expect(session.accept(prefix)).toMatchObject({ text: '', correction: null, pending: true })
    const completed = session.accept(replacement)

    expect(completed.text).toBe(expected)
    expect(completed.correction?.replacement).toBe(expected)
    expect(completed.pending).toBe(false)
  })

  it('supports a longer correction split across three turns', () => {
    const session = new CorrectionSession()

    expect(session.accept('No, así no dije')).toMatchObject({ pending: true })
    expect(session.accept('lo que quería decir era')).toMatchObject({ pending: true })

    const completed = session.accept('la reunión del viernes')
    expect(completed.text).toBe('La reunión del viernes')
    expect(completed.correction?.replacement).toBe('La reunión del viernes')
  })

  it('flushes a false correction prefix as normal transcript', () => {
    const session = new CorrectionSession()

    session.accept('Uy, así no era')
    expect(session.accept('pero continuemos con el plan')).toEqual({
      text: 'Uy, así no era pero continuemos con el plan',
      correction: null,
      pending: false,
    })
  })

  it('resets a pending correction without leaking it into the next note', () => {
    const session = new CorrectionSession()

    session.accept('Uy, así no era')
    session.reset()

    expect(session.accept('La reunión empieza a las diez')).toEqual({
      text: 'La reunión empieza a las diez',
      correction: null,
      pending: false,
    })
  })
})
