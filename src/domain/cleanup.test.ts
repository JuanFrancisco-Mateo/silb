import { describe, expect, it } from 'vitest'
import { cleanTranscript } from './cleanup'

describe('muletillas en español', () => {
  it('elimina "eh" en medio de la frase', () => {
    expect(cleanTranscript('Vamos a empezar, eh, con la reunión')).toEqual({
      text: 'Vamos a empezar, con la reunión',
      removed: ['eh'],
    })
  })

  it('elimina "eh" al final sin coma', () => {
    expect(cleanTranscript('Avísame luego eh')).toEqual({
      text: 'Avísame luego',
      removed: ['eh'],
    })
  })

  it('elimina "eh" conservando el punto final de la frase', () => {
    expect(cleanTranscript('Vamos, eh.')).toEqual({
      text: 'Vamos.',
      removed: ['eh'],
    })
  })

  it('elimina "um" en medio de la frase', () => {
    expect(cleanTranscript('No sé, um, qué hacer')).toEqual({
      text: 'No sé, qué hacer',
      removed: ['um'],
    })
  })

  it('elimina "este" cuando va seguido de pausa', () => {
    expect(cleanTranscript('Este, el informe del jueves')).toEqual({
      text: 'El informe del jueves',
      removed: ['Este'],
    })
  })

  it('respeta "este" como demostrativo legítimo', () => {
    expect(cleanTranscript('Este es el plan')).toEqual({
      text: 'Este es el plan',
      removed: [],
    })
  })

  it('elimina "esto" cuando va seguido de pausa', () => {
    expect(cleanTranscript('Dijo, esto, que vendría')).toEqual({
      text: 'Dijo, que vendría',
      removed: ['esto'],
    })
  })

  it('respeta "esto" como demostrativo legítimo', () => {
    expect(cleanTranscript('Esto es importante')).toEqual({
      text: 'Esto es importante',
      removed: [],
    })
  })

  it('elimina "o sea"', () => {
    expect(cleanTranscript('Vamos, o sea, al parque')).toEqual({
      text: 'Vamos, al parque',
      removed: ['o sea'],
    })
  })

  it('elimina "tipo" cuando va seguido de pausa', () => {
    expect(cleanTranscript('Fue, tipo, muy raro')).toEqual({
      text: 'Fue, muy raro',
      removed: ['tipo'],
    })
  })

  it('respeta "tipo" en "tipo de documento"', () => {
    expect(cleanTranscript('Necesito un tipo de documento')).toEqual({
      text: 'Necesito un tipo de documento',
      removed: [],
    })
  })

  it('elimina "bueno" al inicio y capitaliza la frase', () => {
    expect(cleanTranscript('Bueno, vamos a empezar')).toEqual({
      text: 'Vamos a empezar',
      removed: ['Bueno'],
    })
  })

  it('respeta "bueno" como adjetivo en medio de la frase', () => {
    expect(cleanTranscript('El vino es bueno')).toEqual({
      text: 'El vino es bueno',
      removed: [],
    })
  })

  it('respeta "Bueno." como respuesta de una palabra', () => {
    expect(cleanTranscript('Bueno.')).toEqual({
      text: 'Bueno.',
      removed: [],
    })
  })

  it('elimina "pues" al inicio y capitaliza la frase', () => {
    expect(cleanTranscript('Pues, vamos a ver qué pasa')).toEqual({
      text: 'Vamos a ver qué pasa',
      removed: ['Pues'],
    })
  })

  it('elimina "pues" entre pausas', () => {
    expect(cleanTranscript('Es, pues, complicado')).toEqual({
      text: 'Es, complicado',
      removed: ['pues'],
    })
  })

  it('respeta "pues" con sentido de causa', () => {
    expect(cleanTranscript('Lo hice pues era necesario')).toEqual({
      text: 'Lo hice pues era necesario',
      removed: [],
    })
  })
})

describe('muletillas en inglés', () => {
  it('elimina "like" entre comas', () => {
    expect(cleanTranscript('It was, like, amazing')).toEqual({
      text: 'It was, amazing',
      removed: ['like'],
    })
  })

  it('respeta "like" como verbo', () => {
    expect(cleanTranscript('I like pizza')).toEqual({
      text: 'I like pizza',
      removed: [],
    })
  })

  it('elimina "uh" al inicio y capitaliza la frase', () => {
    expect(cleanTranscript('Uh, I do not know')).toEqual({
      text: 'I do not know',
      removed: ['Uh'],
    })
  })

  it('elimina "um" en medio de la frase', () => {
    expect(cleanTranscript('I mean, um, it is fine')).toEqual({
      text: 'I mean, it is fine',
      removed: ['um'],
    })
  })

  it('elimina "you know"', () => {
    expect(cleanTranscript("It's, you know, complicated")).toEqual({
      text: "It's, complicated",
      removed: ['you know'],
    })
  })
})

describe('repeticiones de palabra y bigrama', () => {
  it('colapsa "el el" al inicio y capitaliza la frase', () => {
    expect(cleanTranscript('el el informe está listo')).toEqual({
      text: 'El informe está listo',
      removed: ['el'],
    })
  })

  it('colapsa "con con" en medio de la frase', () => {
    expect(cleanTranscript('la reunión con con Javier')).toEqual({
      text: 'la reunión con Javier',
      removed: ['con'],
    })
  })

  it('colapsa "que que"', () => {
    expect(cleanTranscript('dijo que que vendría')).toEqual({
      text: 'dijo que vendría',
      removed: ['que'],
    })
  })

  it('colapsa repetición triple "la la la" a una sola ocurrencia', () => {
    expect(cleanTranscript('la la la casa es grande')).toEqual({
      text: 'La casa es grande',
      removed: ['la'],
    })
  })

  it('colapsa bigrama repetido "el informe el informe"', () => {
    expect(cleanTranscript('el informe el informe está listo')).toEqual({
      text: 'El informe está listo',
      removed: ['el informe'],
    })
  })

  it('colapsa repetición en inglés "the the"', () => {
    expect(cleanTranscript('the the report is ready')).toEqual({
      text: 'The report is ready',
      removed: ['the'],
    })
  })
})

describe('frases limpias y edge cases', () => {
  it('deja intacta una frase sin muletillas', () => {
    expect(cleanTranscript('Vamos a preparar el informe del jueves')).toEqual({
      text: 'Vamos a preparar el informe del jueves',
      removed: [],
    })
  })

  it('no reescribe una frase que empieza en minúscula', () => {
    expect(cleanTranscript('vamos a estar ahí')).toEqual({
      text: 'vamos a estar ahí',
      removed: [],
    })
  })

  it('es insensible a mayúsculas', () => {
    expect(cleanTranscript('Dijo EH y O SEA nada')).toEqual({
      text: 'Dijo y nada',
      removed: ['EH', 'O SEA'],
    })
  })

  it('preserva la puntuación de apertura', () => {
    expect(cleanTranscript('¿Eh, vas a venir?')).toEqual({
      text: '¿Vas a venir?',
      removed: ['Eh'],
    })
  })

  it('elimina varias muletillas en el mismo turno', () => {
    expect(cleanTranscript('Bueno, eh, vamos a empezar')).toEqual({
      text: 'Vamos a empezar',
      removed: ['eh', 'Bueno'],
    })
  })

  it('no toca "um" dentro de palabras reales', () => {
    expect(cleanTranscript('número y álbum')).toEqual({
      text: 'número y álbum',
      removed: [],
    })
  })

  it('devuelve vacío para un string vacío', () => {
    expect(cleanTranscript('')).toEqual({ text: '', removed: [] })
  })

  it('devuelve vacío para un string de solo espacios', () => {
    expect(cleanTranscript('   ')).toEqual({ text: '', removed: [] })
  })

  it('normaliza espacios sobrantes', () => {
    expect(cleanTranscript('Bueno,   vamos   a empezar')).toEqual({
      text: 'Vamos a empezar',
      removed: ['Bueno'],
    })
  })
})