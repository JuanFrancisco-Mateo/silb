# Política de seguridad

Silb procesa tu voz **en tu máquina**: la transcripción es local (whisper.cpp) y
la app no manda audio a ningún servidor. La única excepción es la corrección con
IA opcional, y solo si configuras un proveedor en la nube o uno propio.

## Reportar una vulnerabilidad

Si encuentras una vulnerabilidad (que se escriba texto donde no toca, que se
escapen datos, una API key guardada sin proteger, ejecución remota…), **no abras
un issue público**. Usa el
[aviso de vulnerabilidades de GitHub](https://github.com/JuanFrancisco-Mateo/silb/security/advisories/new)
o escribe al mantenedor desde su [perfil de GitHub](https://github.com/JuanFrancisco-Mateo).

Intentaremos responder en unos días y arreglarlo de común acuerdo antes de hacer
nada público.

## Compromisos de privacidad del código

- La API key del proveedor de corrección se guarda en el **Administrador de
  credenciales de Windows**, nunca en `config.json` ni en el registro.
- El registro va rotando por tamaño y no guarda transcripciones.
- La app funciona sin cuenta, sin datos de uso y sin internet (salvo la
  corrección con IA opcional y la descarga de modelos de voz, que empiezas tú).
