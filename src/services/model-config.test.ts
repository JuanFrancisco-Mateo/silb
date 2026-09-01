import { beforeEach, describe, expect, it } from 'vitest'
import { getModel, isModelId, MODEL_OPTIONS, MODEL_STORAGE_KEY, setModel } from './model-config'

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

describe('MODEL_OPTIONS', () => {
  it('expone tiny, base y small con labels en español', () => {
    expect(MODEL_OPTIONS.map((option) => option.id)).toEqual(['tiny', 'base', 'small'])
    expect(MODEL_OPTIONS[0]).toEqual({ id: 'tiny', label: 'Tiny (rápido)' })
    expect(MODEL_OPTIONS[2]).toEqual({ id: 'small', label: 'Small (preciso)' })
  })

  it('isModelId valida ids conocidos y rechaza otros', () => {
    expect(isModelId('tiny')).toBe(true)
    expect(isModelId('base')).toBe(true)
    expect(isModelId('small')).toBe(true)
    expect(isModelId('huge')).toBe(false)
    expect(isModelId(42)).toBe(false)
    expect(isModelId(null)).toBe(false)
  })
})

describe('getModel', () => {
  beforeEach(() => {
    Object.defineProperty(globalThis, 'localStorage', {
      configurable: true,
      value: createLocalStorage(),
    })
  })

  it('devuelve tiny por defecto si no hay nada guardado', () => {
    expect(getModel()).toBe('tiny')
  })

  it('devuelve tiny si el valor guardado es desconocido', () => {
    localStorage.setItem(MODEL_STORAGE_KEY, 'huge')
    expect(getModel()).toBe('tiny')
  })

  it('devuelve el modelo guardado si es válido', () => {
    localStorage.setItem(MODEL_STORAGE_KEY, 'small')
    expect(getModel()).toBe('small')
  })

  it('no lanza si el valor guardado no es un string', () => {
    localStorage.setItem(MODEL_STORAGE_KEY, JSON.stringify({ cualquier: 'cosa' }))
    expect(getModel()).toBe('tiny')
  })

  it('devuelve tiny si localStorage no está disponible (try/catch)', () => {
    Object.defineProperty(globalThis, 'localStorage', { configurable: true, value: undefined })
    expect(getModel()).toBe('tiny')
  })
})

describe('setModel', () => {
  beforeEach(() => {
    Object.defineProperty(globalThis, 'localStorage', {
      configurable: true,
      value: createLocalStorage(),
    })
  })

  it('persiste el modelo y getModel lo recupera', () => {
    expect(setModel('base')).toBe(true)
    expect(localStorage.getItem(MODEL_STORAGE_KEY)).toBe('base')
    expect(getModel()).toBe('base')
  })

  it('persiste small sin problemas', () => {
    expect(setModel('small')).toBe(true)
    expect(getModel()).toBe('small')
  })

  it('rechaza un id desconocido y no escribe nada', () => {
    expect(setModel('huge' as never)).toBe(false)
    expect(localStorage.getItem(MODEL_STORAGE_KEY)).toBeNull()
    expect(getModel()).toBe('tiny')
  })

  it('devuelve false si localStorage.setItem lanza', () => {
    Object.defineProperty(globalThis, 'localStorage', {
      configurable: true,
      value: {
        setItem() {
          throw new Error('QuotaExceededError')
        },
      },
    })
    expect(setModel('tiny')).toBe(false)
  })
})