import { beforeEach, describe, expect, it, vi } from 'vitest'
import { injectTextIntoActiveApp } from './text-injection'

type InvokeMock = ReturnType<typeof vi.fn>

describe('injectTextIntoActiveApp', () => {
  beforeEach(() => {
    // Entorno de pruebas node: creamos un `window` mínimo por test.
    Object.defineProperty(globalThis, 'window', {
      configurable: true,
      writable: true,
      value: {},
    })
  })

  it('devuelve fallback web silencioso si no hay internals de Tauri', async () => {
    await expect(injectTextIntoActiveApp('hola')).resolves.toEqual({ ok: false, error: 'web' })
  })

  it('devuelve fallback web si no existe el objeto window', async () => {
    delete (globalThis as { window?: unknown }).window
    await expect(injectTextIntoActiveApp('hola')).resolves.toEqual({ ok: false, error: 'web' })
  })

  it('devuelve fallback web si los internals no exponen invoke', async () => {
    window.__TAURI_INTERNALS__ = {}
    await expect(injectTextIntoActiveApp('hola')).resolves.toEqual({ ok: false, error: 'web' })
  })

  it('invoca type_text con el texto y resuelve ok', async () => {
    const invoke: InvokeMock = vi.fn().mockResolvedValue(undefined)
    window.__TAURI_INTERNALS__ = { invoke }

    await expect(injectTextIntoActiveApp('Hola mundo')).resolves.toEqual({ ok: true })
    expect(invoke).toHaveBeenCalledOnce()
    expect(invoke).toHaveBeenCalledWith('type_text', { text: 'Hola mundo' })
  })

  it('propaga el error del comando nativo', async () => {
    const invoke: InvokeMock = vi.fn().mockRejectedValue(new Error('no se pudo abrir el portapapeles'))
    window.__TAURI_INTERNALS__ = { invoke }

    await expect(injectTextIntoActiveApp('fallo')).resolves.toEqual({
      ok: false,
      error: 'no se pudo abrir el portapapeles',
    })
  })
})