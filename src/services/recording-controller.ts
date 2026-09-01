import type { WhisperWebLanguage } from '@remotion/whisper-web'

export type RecordingState = 'idle' | 'loading' | 'recording' | 'transcribing' | 'error'

export type TrackLike = { stop: () => void }

export type StreamLike = { getTracks: () => TrackLike[] }

export type RecorderEvent = { data: Blob }

export type RecorderLike = {
  state: string
  mimeType: string
  onstart: ((...args: any[]) => void) | null
  ondataavailable: ((event: RecorderEvent) => void) | null
  onerror: ((...args: any[]) => void) | null
  onstop: ((...args: any[]) => void) | null
  start: (timeslice?: number) => void
  stop: () => void
}

export type RecordingControllerDependencies = {
  getUserMedia: (constraints: { audio: boolean }) => Promise<StreamLike>
  createRecorder: (stream: StreamLike, options?: { mimeType?: string }) => RecorderLike
  isTypeSupported: (mimeType: string) => boolean
  ensureLocalWhisper: (onProgress: (progress: number) => void) => Promise<void>
  transcribeLocalAudio: (
    audio: Blob,
    language: WhisperWebLanguage,
    onProgress: (progress: number) => void,
  ) => Promise<string>
}

type RecordingControllerCallbacks = {
  onState: (state: RecordingState) => void
  onProgress: (progress: number) => void
  onNotice: (notice: string) => void
  onTranscript: (transcript: string) => void
}

export type RecordingController = {
  setLanguage: (language: WhisperWebLanguage) => void
  start: () => Promise<void>
  stop: () => void
  dispose: () => void
}

export function createRecordingController(
  dependencies: RecordingControllerDependencies,
  callbacks: RecordingControllerCallbacks,
  initialLanguage: WhisperWebLanguage,
): RecordingController {
  let language = initialLanguage
  let recorder: RecorderLike | null = null
  let stream: StreamLike | null = null
  let chunks: Blob[] = []
  let state: RecordingState = 'idle'
  let disposed = false

  const setState = (next: RecordingState) => {
    state = next
    callbacks.onState(next)
  }

  const stopTracks = () => {
    stream?.getTracks().forEach((track) => track.stop())
    stream = null
  }

  const fail = (notice: string) => {
    stopTracks()
    recorder = null
    chunks = []
    setState('error')
    callbacks.onNotice(notice)
  }

  const handleStop = () => {
    stopTracks()
    recorder = null

    const audio = new Blob(chunks, { type: chunks[0]?.type || 'audio/webm' })
    chunks = []
    if (disposed) return
    if (audio.size === 0) {
      setState('idle')
      callbacks.onNotice('No se capturó audio')
      return
    }

    setState('transcribing')
    callbacks.onProgress(0)
    callbacks.onNotice('Transcribiendo con Whisper local...')
    void dependencies.transcribeLocalAudio(audio, language, callbacks.onProgress)
      .then((transcript) => {
        if (disposed) return
        if (transcript) callbacks.onTranscript(transcript)
        setState('idle')
        callbacks.onProgress(1)
        callbacks.onNotice(transcript ? 'Texto actualizado con Whisper local' : 'No se detectó voz')
      })
      .catch((error: unknown) => {
        if (disposed) return
        setState('error')
        callbacks.onNotice(error instanceof Error ? error.message : 'La transcripción local falló')
      })
  }

  const start = async () => {
    if (disposed || state === 'loading' || state === 'recording' || state === 'transcribing') return

    try {
      setState('loading')
      callbacks.onProgress(0)
      callbacks.onNotice('Preparando modelo Whisper local...')
      await dependencies.ensureLocalWhisper(callbacks.onProgress)
      if (disposed) return

      stream = await dependencies.getUserMedia({ audio: true })
      const mimeType = dependencies.isTypeSupported('audio/webm;codecs=opus')
        ? 'audio/webm;codecs=opus'
        : ''
      recorder = dependencies.createRecorder(stream, mimeType ? { mimeType } : undefined)
      chunks = []
      recorder.onstart = () => {
        if (disposed) return
        setState('recording')
        callbacks.onProgress(0)
        callbacks.onNotice('Grabando en este dispositivo')
      }
      recorder.ondataavailable = (event) => {
        if (event.data.size > 0) chunks.push(event.data)
      }
      recorder.onerror = () => fail('No se pudo leer el audio del micrófono')
      recorder.onstop = handleStop
      recorder.start(250)
    } catch (error: unknown) {
      if (disposed) return
      fail(error instanceof Error ? error.message : 'No se pudo iniciar Whisper local')
    }
  }

  return {
    setLanguage: (nextLanguage) => {
      language = nextLanguage
    },
    start,
    stop: () => {
      if (recorder && recorder.state !== 'inactive') {
        callbacks.onNotice('Procesando audio local...')
        recorder.stop()
      }
    },
    dispose: () => {
      disposed = true
      if (recorder && recorder.state !== 'inactive') recorder.stop()
      stopTracks()
      recorder = null
      chunks = []
      state = 'idle'
    },
  }
}
