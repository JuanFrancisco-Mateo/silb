# Política de seguridad

Whispr Open procesa tu voz **en tu máquina**: la transcripción es local
(whisper.cpp) y la app no envía audio a ningún servidor. La única excepción es
la corrección IA opcional, y solo si configuras un proveedor Cloud/Custom.

## Reportar una vulnerabilidad

Si encuentras una vulnerabilidad (inyección de texto indebida, fuga de datos,
API key en texto plano, ejecución remota…), **no abras un issue público**.
Usa la [vulnerability reporting de GitHub](https://github.com/JuanFrancisco-Mateo/whispr-open/security/advisories/new)
o contacta con el mantenedor vía su [perfil de GitHub](https://github.com/JuanFrancisco-Mateo).

Intentaremos responder en unos días y coordinar la corrección antes de hacer
nada público.

## Compromisos de privacidad del código

- La API key del proveedor de corrección se guarda en el **Credential Manager
  de Windows**, nunca en `config.json` ni en el log.
- El log rota por tamaño y no guarda transcripciones.
- La app funciona sin cuenta, sin telemetría y sin red (salvo la corrección IA
  opcional y la descarga de modelos de voz que tú inicias).
