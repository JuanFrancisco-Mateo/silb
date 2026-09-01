import { beforeEach, describe, expect, it } from 'vitest'
import {
  applyDictionary,
  DICTIONARY_STORAGE_KEY,
  loadDictionary,
  saveDictionary,
  type DictionaryEntry,
} from './dictionary'

function createLocalStorage(): Storage {
  const store = new Map<string, string>()
  return {
    get length() {
      return store.size
    },
    clear() {
      store.clear()
    },
    getItem(key: string) {
      return store.get(key) ?? null
    },
    key(index: number) {
      return [...store.keys()][index] ?? null
    },
    removeItem(key: string) {
      store.delete(key)
    },
    setItem(key: string, value: string) {
      store.set(key, String(value))
    },
  }
}

describe('applyDictionary', () => {
  it('reemplaza un término y lo reporta en applied', () => {
    const result = applyDictionary('mándalo por whatsapp', [{ from: 'whatsapp', to: 'WhatsApp' }])
    expect(result.text).toBe('mándalo por WhatsApp')
    expect(result.applied).toEqual(['whatsapp'])
  })

  it('hace match case-insensitive tanto en el texto como en el término', () => {
    const result = applyDictionary('HE VISTO OpenAI y openai', [{ from: 'openai', to: 'OpenAI' }])
    expect(result.text).toBe('HE VISTO OpenAI y OpenAI')
    expect(result.applied).toEqual(['openai'])
  })

  it('no reemplaza subpalabras (exige palabra completa)', () => {
    const result = applyDictionary('quiero whatsappear ahora', [{ from: 'whatsapp', to: 'WhatsApp' }])
    expect(result.text).toBe('quiero whatsappear ahora')
    expect(result.applied).toEqual([])
  })

  it('no reemplaza dentro de otra palabra', () => {
    const result = applyDictionary('una añoranza antigua', [{ from: 'año', to: 'anio' }])
    expect(result.text).toBe('una añoranza antigua')
    expect(result.applied).toEqual([])
  })

  it('respeta puntuación adyacente a la palabra', () => {
    const result = applyDictionary('dime, whatsapp, por favor', [{ from: 'whatsapp', to: 'WhatsApp' }])
    expect(result.text).toBe('dime, WhatsApp, por favor')
    expect(result.applied).toEqual(['whatsapp'])
  })

  it('reemplaza términos multi-palabra', () => {
    const result = applyDictionary('usa open ai para esto', [{ from: 'open ai', to: 'OpenAI' }])
    expect(result.text).toBe('usa OpenAI para esto')
    expect(result.applied).toEqual(['open ai'])
  })

  it('aplica varias entradas en orden sobre el resultado previo', () => {
    const entries: DictionaryEntry[] = [
      { from: 'whatsapp', to: 'WhatsApp' },
      { from: 'lidl', to: 'LIDR' },
    ]
    const result = applyDictionary('whatsapp y lidl', entries)
    expect(result.text).toBe('WhatsApp y LIDR')
    expect(result.applied).toEqual(['whatsapp', 'lidl'])
  })

  it('reporta applied una sola vez si el término aparece repetido', () => {
    const result = applyDictionary('whatsapp y whatsapp', [{ from: 'whatsapp', to: 'WhatsApp' }])
    expect(result.text).toBe('WhatsApp y WhatsApp')
    expect(result.applied).toEqual(['whatsapp'])
  })

  it('devuelve el texto intacto si no hay coincidencias', () => {
    const result = applyDictionary('texto sin términos', [{ from: 'openai', to: 'OpenAI' }])
    expect(result.text).toBe('texto sin términos')
    expect(result.applied).toEqual([])
  })

  it('ignora entradas con from vacío', () => {
    const result = applyDictionary('hola mundo', [{ from: '   ', to: 'X' }])
    expect(result.text).toBe('hola mundo')
    expect(result.applied).toEqual([])
  })

  it('maneja texto vacío', () => {
    const result = applyDictionary('', [{ from: 'whatsapp', to: 'WhatsApp' }])
    expect(result.text).toBe('')
    expect(result.applied).toEqual([])
  })

  it('maneja lista de entradas vacía', () => {
    const result = applyDictionary('texto cualquiera', [])
    expect(result.text).toBe('texto cualquiera')
    expect(result.applied).toEqual([])
  })

  it('escapa caracteres especiales del término', () => {
    const result = applyDictionary('sé programar en c++', [{ from: 'c++', to: 'C++' }])
    expect(result.text).toBe('sé programar en C++')
    expect(result.applied).toEqual(['c++'])
  })

  it('respeta palabras con acentos en los límites', () => {
    const result = applyDictionary('un café con café', [{ from: 'café', to: 'COFFEE' }])
    expect(result.text).toBe('un COFFEE con COFFEE')
    expect(result.applied).toEqual(['café'])
  })

  it('no vuelve a tocar un término ya transformado por otra entrada', () => {
    const entries: DictionaryEntry[] = [
      { from: 'open ai', to: 'OpenAI' },
      { from: 'ai', to: 'XX' },
    ]
    const result = applyDictionary('usa open ai', entries)
    expect(result.text).toBe('usa OpenAI')
    expect(result.applied).toEqual(['open ai'])
  })
})

describe('persistencia del diccionario', () => {
  beforeEach(() => {
    Object.defineProperty(globalThis, 'localStorage', {
      configurable: true,
      value: createLocalStorage(),
    })
  })

  it('guarda y recupera las entradas en localStorage', () => {
    const entries: DictionaryEntry[] = [
      { from: 'whatsapp', to: 'WhatsApp' },
      { from: 'open ai', to: 'OpenAI' },
    ]
    expect(saveDictionary(entries)).toBe(true)
    expect(localStorage.getItem(DICTIONARY_STORAGE_KEY)).toBe(JSON.stringify(entries))
    expect(loadDictionary()).toEqual(entries)
  })

  it('devuelve [] si no hay nada guardado', () => {
    expect(loadDictionary()).toEqual([])
  })

  it('devuelve [] y no lanza ante JSON corrupto', () => {
    localStorage.setItem(DICTIONARY_STORAGE_KEY, '{esto-no-es-json')
    expect(loadDictionary()).toEqual([])
  })

  it('devuelve [] si el valor guardado no es un array', () => {
    localStorage.setItem(DICTIONARY_STORAGE_KEY, JSON.stringify({ from: 'whatsapp', to: 'WhatsApp' }))
    expect(loadDictionary()).toEqual([])
  })

  it('filtra entradas malformadas del array guardado', () => {
    localStorage.setItem(
      DICTIONARY_STORAGE_KEY,
      JSON.stringify([
        { from: 'whatsapp', to: 'WhatsApp' },
        { from: 42, to: 'X' },
        { from: 'solo-from' },
        null,
      ]),
    )
    expect(loadDictionary()).toEqual([{ from: 'whatsapp', to: 'WhatsApp' }])
  })

  it('saveDictionary devuelve false si setItem lanza', () => {
    Object.defineProperty(globalThis, 'localStorage', {
      configurable: true,
      value: {
        setItem() {
          throw new Error('QuotaExceededError')
        },
      },
    })
    expect(saveDictionary([{ from: 'a', to: 'b' }])).toBe(false)
  })

  it('loadDictionary devuelve [] si localStorage no existe (try/catch)', () => {
    Object.defineProperty(globalThis, 'localStorage', { configurable: true, value: undefined })
    expect(loadDictionary()).toEqual([])
  })
})