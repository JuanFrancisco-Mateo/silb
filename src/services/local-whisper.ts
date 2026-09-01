import {
  canUseWhisperWeb,
  downloadWhisperModel,
  resampleTo16Khz,
  transcribe,
  type WhisperWebLanguage,
  type WhisperWebModel,
} from '@remotion/whisper-web'

export const LOCAL_WHISPER_MODEL: WhisperWebModel = 'tiny'

type ProgressHandler = (progress: number) => void

let modelReady = false
let modelPromise: Promise<void> | null = null

export function resetLocalWhisperForTests() {
  modelReady = false
  modelPromise = null
}

export async function ensureLocalWhisper(onProgress: ProgressHandler) {
  if (modelReady) return
  if (!modelPromise) {
    modelPromise = (async () => {
      const support = await canUseWhisperWeb(LOCAL_WHISPER_MODEL)
      if (!support.supported) {
        throw new Error(support.detailedReason ?? 'Este navegador no puede ejecutar Whisper localmente.')
      }

      await downloadWhisperModel({
        model: LOCAL_WHISPER_MODEL,
        onProgress: ({ progress }) => onProgress(progress),
      })
      modelReady = true
    })().catch((error) => {
      modelPromise = null
      throw error
    })
  }

  await modelPromise
}

export async function transcribeLocalAudio(
  audio: Blob,
  language: WhisperWebLanguage,
  onProgress: ProgressHandler,
) {
  const channelWaveform = await resampleTo16Khz({
    file: audio,
    onProgress: (progress) => onProgress(progress * 0.2),
    logLevel: 'error',
  })

  const result = await transcribe({
    channelWaveform,
    model: LOCAL_WHISPER_MODEL,
    language,
    threads: Math.min(4, navigator.hardwareConcurrency || 4),
    onProgress: (progress) => onProgress(0.2 + progress * 0.8),
    logLevel: 'error',
  })

  return result.transcription.map((segment) => segment.text).join(' ').replace(/\s+/g, ' ').trim()
}
