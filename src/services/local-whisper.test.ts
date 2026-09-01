import { beforeEach, describe, expect, it, vi } from 'vitest'
import { downloadWhisperModel, canUseWhisperWeb, resampleTo16Khz, transcribe } from '@remotion/whisper-web'
import { ensureLocalWhisper, resetLocalWhisperForTests, transcribeLocalAudio } from './local-whisper'

vi.mock('@remotion/whisper-web', () => ({
  canUseWhisperWeb: vi.fn(),
  downloadWhisperModel: vi.fn(),
  resampleTo16Khz: vi.fn(),
  transcribe: vi.fn(),
}))

const mockedCanUse = vi.mocked(canUseWhisperWeb)
const mockedDownload = vi.mocked(downloadWhisperModel)
const mockedResample = vi.mocked(resampleTo16Khz)
const mockedTranscribe = vi.mocked(transcribe)

describe('local Whisper service', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    resetLocalWhisperForTests()
    mockedCanUse.mockResolvedValue({ supported: true })
    mockedDownload.mockImplementation(async ({ onProgress }) => {
      onProgress({ downloadedBytes: 74000000, totalBytes: 74000000, progress: 1 })
      return { alreadyDownloaded: false }
    })
    mockedResample.mockImplementation(async ({ onProgress }) => {
      onProgress?.(1)
      return new Float32Array([0, 0.25, -0.25])
    })
    mockedTranscribe.mockImplementation(async ({ onProgress }) => {
      onProgress?.(0.75)
      return {
        systeminfo: 'test',
        model: {},
        result: {},
        transcription: [
          { text: ' Hola', offsets: { from: 0, to: 100 }, timestamps: { from: '0', to: '0.1' }, tokens: [] },
          { text: ' mundo ', offsets: { from: 100, to: 200 }, timestamps: { from: '0.1', to: '0.2' }, tokens: [] },
        ],
      } as unknown as Awaited<ReturnType<typeof transcribe>>
    })
    Object.defineProperty(globalThis, 'navigator', {
      configurable: true,
      value: { hardwareConcurrency: 8 },
    })
  })

  it('checks support and downloads the tiny model once', async () => {
    const progress: number[] = []

    await ensureLocalWhisper((value) => progress.push(value))
    await ensureLocalWhisper((value) => progress.push(value))

    expect(mockedCanUse).toHaveBeenCalledOnce()
    expect(mockedDownload).toHaveBeenCalledOnce()
    expect(mockedDownload.mock.calls[0][0].model).toBe('tiny')
    expect(progress).toContain(1)
  })

  it('resamples audio and returns normalized local transcription', async () => {
    const progress: number[] = []
    const audio = new Blob(['audio fixture'], { type: 'audio/webm' })

    const result = await transcribeLocalAudio(audio, 'es', (value) => progress.push(value))

    expect(mockedResample).toHaveBeenCalledWith(expect.objectContaining({ file: audio, logLevel: 'error' }))
    expect(mockedTranscribe).toHaveBeenCalledWith(expect.objectContaining({
      channelWaveform: expect.any(Float32Array),
      model: 'tiny',
      language: 'es',
      threads: 4,
      logLevel: 'error',
    }))
    expect(result).toBe('Hola mundo')
    expect(progress).toContain(0.2)
    expect(progress).toContain(0.8)
  })

  it('surfaces unsupported browser diagnostics', async () => {
    mockedCanUse.mockResolvedValue({
      supported: false,
      detailedReason: 'SharedArrayBuffer no disponible',
    })

    await expect(ensureLocalWhisper(() => undefined)).rejects.toThrow('SharedArrayBuffer no disponible')
    expect(mockedDownload).not.toHaveBeenCalled()
  })
})
