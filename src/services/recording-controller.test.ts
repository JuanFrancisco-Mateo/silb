import { describe, expect, it, vi } from 'vitest'
import type { WhisperWebLanguage } from '@remotion/whisper-web'
import {
  createRecordingController,
  type RecorderLike,
  type StreamLike,
} from './recording-controller'

class FakeRecorder implements RecorderLike {
  state = 'inactive'
  mimeType = 'audio/webm;codecs=opus'
  onstart: RecorderLike['onstart'] = null
  ondataavailable: RecorderLike['ondataavailable'] = null
  onerror: RecorderLike['onerror'] = null
  onstop: RecorderLike['onstop'] = null
  startCalls: number[] = []

  start(timeslice?: number) {
    this.state = 'recording'
    this.startCalls.push(timeslice ?? 0)
    this.onstart?.()
  }

  stop() {
    this.state = 'inactive'
    this.onstop?.()
  }

  emitAudio(text: string) {
    this.ondataavailable?.({ data: new Blob([text], { type: this.mimeType }) })
  }
}

function createStream() {
  const track = { stop: vi.fn() }
  const stream: StreamLike = { getTracks: () => [track] }
  return { stream, track }
}

function createHarness(transcript = 'Texto transcrito') {
  const recorder = new FakeRecorder()
  const { stream, track } = createStream()
  const ensureLocalWhisper = vi.fn(async (onProgress: (progress: number) => void) => {
    onProgress(0.5)
  })
  const transcribeLocalAudio = vi.fn(async (
    _audio: Blob,
    _language: WhisperWebLanguage,
    onProgress: (progress: number) => void,
  ) => {
    onProgress(0.75)
    return transcript
  })
  const states: string[] = []
  const notices: string[] = []
  const progress: number[] = []
  const transcripts: string[] = []

  const controller = createRecordingController({
    getUserMedia: vi.fn(async () => stream),
    createRecorder: vi.fn(() => recorder),
    isTypeSupported: vi.fn(() => true),
    ensureLocalWhisper,
    transcribeLocalAudio,
  }, {
    onState: (state) => states.push(state),
    onProgress: (value) => progress.push(value),
    onNotice: (notice) => notices.push(notice),
    onTranscript: (value) => transcripts.push(value),
  }, 'es')

  return {
    controller,
    recorder,
    track,
    ensureLocalWhisper,
    transcribeLocalAudio,
    states,
    notices,
    progress,
    transcripts,
  }
}

describe('createRecordingController', () => {
  it('records a turn, transcribes it locally, and releases the microphone', async () => {
    const harness = createHarness()

    await harness.controller.start()
    harness.recorder.emitAudio('audio fixture')
    harness.controller.stop()
    await Promise.resolve()

    expect(harness.ensureLocalWhisper).toHaveBeenCalledOnce()
    expect(harness.recorder.startCalls).toEqual([250])
    expect(harness.states).toEqual(['loading', 'recording', 'transcribing', 'idle'])
    expect(harness.transcribeLocalAudio).toHaveBeenCalledOnce()
    expect(harness.transcribeLocalAudio.mock.calls[0][1]).toBe('es')
    expect(harness.transcribeLocalAudio.mock.calls[0][0]).toBeInstanceOf(Blob)
    expect(harness.transcripts).toEqual(['Texto transcrito'])
    expect(harness.progress).toContain(0.5)
    expect(harness.progress).toContain(0.75)
    expect(harness.progress[harness.progress.length - 1]).toBe(1)
    expect(harness.track.stop).toHaveBeenCalledOnce()
    expect(harness.notices).toContain('Grabando en este dispositivo')
    expect(harness.notices).toContain('Texto actualizado con Whisper local')
  })

  it('falls back to the browser mime type when opus is unavailable', async () => {
    const harness = createHarness()
    const stream = createStream()
    const createRecorder = vi.fn(() => harness.recorder)
    const getUserMedia = vi.fn(async () => stream.stream)
    const controller = createRecordingController({
      getUserMedia,
      createRecorder,
      isTypeSupported: vi.fn(() => false),
      ensureLocalWhisper: harness.ensureLocalWhisper,
      transcribeLocalAudio: harness.transcribeLocalAudio,
    }, {
      onState: vi.fn(),
      onProgress: vi.fn(),
      onNotice: vi.fn(),
      onTranscript: vi.fn(),
    }, 'en')

    await controller.start()

    expect(createRecorder).toHaveBeenCalledWith(stream.stream, undefined)
    expect(getUserMedia).toHaveBeenCalledWith({ audio: true })
  })

  it('reports model initialization failures and stops the stream', async () => {
    const { stream, track } = createStream()
    const notices: string[] = []
    const controller = createRecordingController({
      getUserMedia: vi.fn(async () => stream),
      createRecorder: vi.fn(),
      isTypeSupported: vi.fn(() => true),
      ensureLocalWhisper: vi.fn(async () => {
        throw new Error('WASM no disponible')
      }),
      transcribeLocalAudio: vi.fn(),
    }, {
      onState: vi.fn(),
      onProgress: vi.fn(),
      onNotice: (notice) => notices.push(notice),
      onTranscript: vi.fn(),
    }, 'es')

    await controller.start()

    expect(notices[notices.length - 1]).toBe('WASM no disponible')
    expect(track.stop).not.toHaveBeenCalled()
  })

  it('does not transcribe empty recordings', async () => {
    const harness = createHarness()

    await harness.controller.start()
    harness.controller.stop()
    await Promise.resolve()

    expect(harness.transcribeLocalAudio).not.toHaveBeenCalled()
    expect(harness.notices).toContain('No se capturó audio')
    expect(harness.track.stop).toHaveBeenCalledOnce()
  })
})
