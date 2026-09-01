import { useCallback, useEffect, useRef, useState } from 'react'
import { appendTranscript, CorrectionSession, replaceLastSentence, type Correction } from './domain/correction'
import { listenForGlobalDictation } from './services/desktop-shortcut'
import { createRecordingController, type RecorderLike, type RecordingState } from './services/recording-controller'
import { ensureLocalWhisper, transcribeLocalAudio } from './services/local-whisper'

const initialText = 'Necesito preparar el informe de producto para la reunión del jueves.'

function App() {
  const [text, setText] = useState(initialText)
  const [isListening, setIsListening] = useState(false)
  const [language, setLanguage] = useState('Español')
  const [corrections, setCorrections] = useState<Correction[]>([])
  const [lastSaved, setLastSaved] = useState('Hace un momento')
  const [notice, setNotice] = useState('Whisper WASM listo bajo demanda')
  const [engineStatus, setEngineStatus] = useState<RecordingState>('idle')
  const [progress, setProgress] = useState(0)
  const correctionSessionRef = useRef(new CorrectionSession())
  const recordingControllerRef = useRef<ReturnType<typeof createRecordingController> | null>(null)

  const processTranscript = useCallback((transcript: string) => {
    const result = correctionSessionRef.current.accept(transcript)
    if (result.pending) {
      setNotice('Rectificación pendiente...')
      return
    }

    setText((current) => result.correction ? replaceLastSentence(current, result.text) : appendTranscript(current, result.text))
    if (result.correction) {
      setCorrections((current) => [result.correction!, ...current].slice(0, 4))
      setNotice('Rectificación aplicada')
    } else {
      setNotice('Texto actualizado')
    }
    setLastSaved('Hace un momento')
  }, [])

  useEffect(() => {
    const controller = createRecordingController({
      getUserMedia: async (constraints) => navigator.mediaDevices.getUserMedia(constraints),
      createRecorder: (stream, options) => new MediaRecorder(stream as MediaStream, options) as unknown as RecorderLike,
      isTypeSupported: (mimeType) => typeof MediaRecorder !== 'undefined' && MediaRecorder.isTypeSupported(mimeType),
      ensureLocalWhisper,
      transcribeLocalAudio,
    }, {
      onState: (state) => {
        setEngineStatus(state)
        setIsListening(state === 'recording')
      },
      onProgress: setProgress,
      onNotice: setNotice,
      onTranscript: processTranscript,
    }, language === 'Español' ? 'es' : 'en')
    recordingControllerRef.current = controller

    return () => {
      controller.dispose()
      recordingControllerRef.current = null
    }
  }, [language, processTranscript])

  const stopListening = useCallback(() => {
    recordingControllerRef.current?.stop()
  }, [])

  const startListening = useCallback(() => {
    if (!navigator.mediaDevices?.getUserMedia || typeof MediaRecorder === 'undefined') {
      setEngineStatus('error')
      setNotice('Este navegador no permite grabación local')
      return
    }
    void recordingControllerRef.current?.start()
  }, [])

  const toggleListening = useCallback(() => {
    if (engineStatus === 'loading' || engineStatus === 'transcribing') return
    if (isListening) stopListening()
    else startListening()
  }, [engineStatus, isListening, startListening, stopListening])

  useEffect(() => {
    let disposed = false
    let unlisten: (() => void) | undefined

    void listenForGlobalDictation(toggleListening).then((cleanup) => {
      if (disposed) cleanup()
      else unlisten = cleanup
    })

    return () => {
      disposed = true
      unlisten?.()
    }
  }, [toggleListening])

  const clearText = () => {
    correctionSessionRef.current.reset()
    setText('')
    setCorrections([])
    setNotice('Texto limpio')
    setLastSaved('Sin guardar')
  }

  const saveDraft = () => {
    localStorage.setItem('whispr-open-draft', text)
    setLastSaved('Guardado ahora')
    setNotice('Borrador guardado localmente')
  }

  const insertDemoCorrection = () => {
    processTranscript('Uy, así no era, era el informe de producto para el jueves')
  }

  return (
    <div className="app-shell">
      <header className="topbar">
        <a className="brand" href="/" aria-label="Whispr Open inicio">
          <span className="brand-mark"><span /></span>
          <span>whispr<span className="brand-muted">/open</span></span>
        </a>
        <div className="topbar-actions">
          <span className="open-badge"><span className="status-dot" /> Open source</span>
          <button className="icon-button" type="button" aria-label="Configuración" title="Configuración">
            <SettingsIcon />
          </button>
          <button className="avatar" type="button" aria-label="Perfil de usuario">AM</button>
        </div>
      </header>

      <main className="workspace">
        <section className="intro-row">
          <div>
            <p className="eyebrow">ESPACIO DE TRABAJO / 01</p>
            <h1>Dictado <span>natural.</span></h1>
            <p className="intro-copy">Habla como piensas. Whispr se encarga del resto.</p>
          </div>
          <div className="privacy-note"><LockIcon /> Borrador guardado en este dispositivo</div>
        </section>

        <section className="editor-layout">
          <div className="editor-column">
            <div className="editor-card">
              <div className="editor-toolbar">
                <div className="document-name"><DocumentIcon /> Nota sin título <ChevronIcon /></div>
                <div className="editor-tools">
                  <span className="saved-label"><span className="saved-dot" /> {lastSaved}</span>
                  <button className="tool-button" type="button" aria-label="Deshacer" title="Deshacer"><UndoIcon /></button>
                  <button className="tool-button" type="button" aria-label="Rehacer" title="Rehacer"><RedoIcon /></button>
                </div>
              </div>
              <textarea
                value={text}
                onChange={(event) => setText(event.target.value)}
                aria-label="Texto dictado"
                placeholder="Empieza a hablar o escribe aquí..."
              />
              <div className="editor-footer">
                <span>{text.length} caracteres</span>
                <span className="language-label"><GlobeIcon /> {language}</span>
              </div>
            </div>

            <div className="control-row">
              <button className={`listen-button ${isListening ? 'listening' : ''}`} type="button" onClick={toggleListening} disabled={engineStatus === 'loading' || engineStatus === 'transcribing'}>
                <span className="mic-wrap"><MicIcon /></span>
                <span>{isListening ? 'Detener dictado' : engineStatus === 'loading' ? 'Preparando motor' : engineStatus === 'transcribing' ? 'Transcribiendo' : 'Empezar a dictar'}</span>
                <span className="shortcut">Ctrl/Cmd ⇧ D</span>
              </button>
              <div className="secondary-actions">
                <button className="secondary-button" type="button" onClick={clearText}><TrashIcon /> Limpiar</button>
                <button className="secondary-button primary-text" type="button" onClick={saveDraft}><SaveIcon /> Guardar</button>
              </div>
            </div>
            <div className={`listening-status ${engineStatus === 'recording' ? 'active' : ''}`}><span className="status-pulse" /> {notice}{(engineStatus === 'loading' || engineStatus === 'transcribing') && <span className="interim"> · {Math.round(progress * 100)}%</span>}</div>
          </div>

          <aside className="activity-panel">
            <div className="panel-heading">
              <div><p className="eyebrow">ASISTENTE</p><h2>Actividad reciente</h2></div>
              <span className="count-badge">{corrections.length || '—'}</span>
            </div>
            <div className="activity-list">
              {corrections.length === 0 ? (
                <div className="empty-activity">
                  <span className="empty-icon"><SparkIcon /></span>
                  <strong>Las correcciones aparecerán aquí</strong>
                  <p>Cuando te rectifiques mientras hablas, Whispr ajustará el texto y te mostrará qué ha cambiado.</p>
                  <button type="button" onClick={insertDemoCorrection}>Probar una rectificación <ArrowIcon /></button>
                </div>
              ) : corrections.map((item) => (
                <div className="activity-item" key={item.id}>
                  <div className="activity-top"><span className="correction-icon"><CheckIcon /></span><span>Corregido · {item.createdAt}</span></div>
                  <p className="original-text">“{item.original}”</p>
                  <p className="replacement-text"><ArrowIcon /> {item.replacement}</p>
                </div>
              ))}
            </div>
            <div className={`engine-note ${engineStatus === 'error' ? 'engine-error' : ''}`}><span className="spark-small"><SparkIcon /></span><div><strong>{engineStatus === 'recording' ? 'Grabando localmente' : engineStatus === 'transcribing' ? 'Whisper procesando' : engineStatus === 'loading' ? 'Cargando modelo local' : engineStatus === 'error' ? 'Motor local necesita atención' : 'Whisper WASM disponible'}</strong><p>Modelo tiny cacheado en IndexedDB. El audio se procesa en este dispositivo.</p>{(engineStatus === 'loading' || engineStatus === 'transcribing') && <span className="engine-progress"><span style={{ width: `${progress * 100}%` }} /></span>}</div></div>
          </aside>
        </section>

        <section className="bottom-strip">
          <div className="strip-item"><span className="strip-icon"><WaveIcon /></span><div><strong>Transcripción en tiempo real</strong><span>Texto provisional mientras hablas</span></div></div>
          <div className="strip-item"><span className="strip-icon"><WandIcon /></span><div><strong>Rectificación inteligente</strong><span>Detecta “así no era” automáticamente</span></div></div>
          <div className="strip-item"><span className="strip-icon"><CodeIcon /></span><div><strong>Atajo global de escritorio</strong><span>Ctrl/Cmd ⇧ D para dictar desde cualquier app</span></div></div>
        </section>
      </main>
    </div>
  )
}

function Icon({ children, viewBox = '0 0 24 24' }: { children: React.ReactNode; viewBox?: string }) {
  return <svg viewBox={viewBox} fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">{children}</svg>
}
const SettingsIcon = () => <Icon><path d="M12 15.5a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7Z" /><path d="m19.4 15 .1.1a2 2 0 0 1-2.8 2.8l-.1-.1a2 2 0 0 0-3.4 1.4v.2a2 2 0 0 1-4 0v-.2a2 2 0 0 0-3.4-1.4l-.1.1A2 2 0 1 1 3 15.1l.1-.1A2 2 0 0 0 1.7 11.6h-.2a2 2 0 0 1 0-4h.2A2 2 0 0 0 3.1 4.2L3 4.1a2 2 0 1 1 2.8-2.8l.1.1A2 2 0 0 0 9.3 0h.2a2 2 0 0 1 4 0h.2a2 2 0 0 0 3.4 1.4l.1-.1A2 2 0 1 1 20 4.1l-.1.1a2 2 0 0 0 1.4 3.4h.2a2 2 0 0 1 0 4h-.2a2 2 0 0 0-1.4 3.4Z" transform="scale(.9) translate(1.3 1.3)" /></Icon>
const LockIcon = () => <Icon><rect x="5" y="10" width="14" height="10" rx="2" /><path d="M8 10V7a4 4 0 0 1 8 0v3" /></Icon>
const DocumentIcon = () => <Icon><path d="M14 3H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9Z" /><path d="M14 3v6h6M8 13h8M8 17h5" /></Icon>
const ChevronIcon = () => <Icon><path d="m7 10 5 5 5-5" /></Icon>
const UndoIcon = () => <Icon><path d="M9 14 4 9l5-5" /><path d="M4 9h10a6 6 0 0 1 6 6v1" /></Icon>
const RedoIcon = () => <Icon><path d="m15 14 5-5-5-5" /><path d="M20 9H10a6 6 0 0 0-6 6v1" /></Icon>
const MicIcon = () => <Icon><rect x="9" y="3" width="6" height="11" rx="3" /><path d="M5 11a7 7 0 0 0 14 0M12 18v3M8 21h8" /></Icon>
const TrashIcon = () => <Icon><path d="M4 7h16M10 11v6M14 11v6M6 7l1 13h10l1-13M9 7V4h6v3" /></Icon>
const SaveIcon = () => <Icon><path d="M5 4h12l2 2v14H5Z" /><path d="M8 4v6h8V4M8 20v-6h8v6" /></Icon>
const GlobeIcon = () => <Icon><circle cx="12" cy="12" r="9" /><path d="M3 12h18M12 3a14 14 0 0 1 0 18M12 3a14 14 0 0 0 0 18" /></Icon>
const SparkIcon = () => <Icon><path d="m12 3 1.3 5.7L19 10l-5.7 1.3L12 17l-1.3-5.7L5 10l5.7-1.3Z" /><path d="m19 16 .6 2.4L22 19l-2.4.6L19 22l-.6-2.4L16 19l2.4-.6Z" /></Icon>
const ArrowIcon = () => <Icon><path d="M5 12h14M13 6l6 6-6 6" /></Icon>
const CheckIcon = () => <Icon><path d="m5 12 4 4L19 6" /></Icon>
const WaveIcon = () => <Icon><path d="M3 12h2l2-6 3 12 3-9 2 6 2-3h5" /></Icon>
const WandIcon = () => <Icon><path d="m15 4 5 5M13 6l5 5M4 20l8-8M3 21l3-1 2-3-2-2-3 2Z" /><path d="m18 2 .5 1.5L20 4l-1.5.5L18 6l-.5-1.5L16 4l1.5-.5Z" /></Icon>
const CodeIcon = () => <Icon><path d="m8 9-4 3 4 3M16 9l4 3-4 3M14 5l-4 14" /></Icon>

export default App
