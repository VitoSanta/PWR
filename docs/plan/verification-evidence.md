# Prove di verifica: scope, freschezza e limiti

Il processo che termina con exit 0 è un dato, non una prova universale del
requisito. PWR conserva stdout, stderr, stato e hash originali. Un riepilogo di
test riconosciuto che dichiara fallimenti impedisce la promozione a verde anche
quando un wrapper restituisce 0. Formati sconosciuti o stdout deliberatamente
mendace non sono autenticati da questa interpretazione.

## Identità e riuso

Ogni nuova `VerificationBaseline` contiene `provenance` versione 1:

- root canonica, argv strutturati e hash della configurazione di esecuzione;
- file nello scope e hash di contenuto prima e dopo i controlli;
- conteggi da riepiloghi riconosciuti (`None` significa sconosciuto, non zero);
- scope dichiarato; ID delle baseline collegati dal confronto e risultati grezzi.

Lo scope comprende file workspace non ignorati, `.pwr/checks.json` e file locali
passati direttamente nei comandi, anche se ignorati. Esclude dipendenze, output
di build, cache e stato PWR; i percorsi esclusi sono elencati in
`pwr-verify/src/provenance.rs`. Non attesta binari/toolchain esterni, servizi,
file ignorati caricati indirettamente o cambiamenti transitori poi ripristinati.
Limiti: 100.000 voci, 64 livelli, 64 MiB per file e 256 MiB complessivi.
Symlink, file speciali, letture fallite e limiti superati non certificano un
inventario parziale. I timestamp non sostituiscono gli hash.

Se i file cambiano durante i controlli o prima del riuso delle prove tramite
`evidence::checks`/confronto, la verifica resta non valida pur conservando il
verde grezzo del processo. Occorre rieseguire i controlli. Una baseline iniziale
instabile non può giustificare una preservazione. Gli esiti storici descrivono
il loro snapshot, non certificano automaticamente il workspace attuale.
Le vecchie registrazioni prive di provenance restano leggibili; non acquisiscono
retroattivamente una tracciabilità mai misurata.

## Criterio indipendente

Per un Goal, il proprietario dichiara il criterio di accettazione e gli artefatti
prima delle azioni. Esempio:

```json
{
  "checks": [
    {"executable": "python3", "args": ["-B", "tests/acceptance.py"], "kind": "acceptance"}
  ],
  "acceptance": {"artifacts": ["tests/acceptance.py"]}
}
```

Una modifica di questi artefatti o della dichiarazione richiede la revisione
umana già prevista dal contratto; non si sostituisce il criterio con i test
proposti dal modello. Le autorizzazioni sono per file e restano registrate.
Il pass tecnico da solo non equivale ad accettazione del Goal. Le regressioni
semantiche S02 mostrano perché: un checker superficiale passa anche su codice
che sbaglia cifre Unicode, interi grandi, booleani o atomicità di un batch.

Un controllo omesso produce baseline `incomplete`, non un test fallito né una
baseline preservata. Nessun check, zero test, interruzione e fallimento restano
esiti distinti. Un flag `accepted` contraddetto da questi dati non rende il
risultato verificato. Build, lint, unit test, browser e artefatti dimostrano
soltanto ciò che controllano: i requisiti fuori dalla copertura rimangono da
provare. I test dev S02 non qualificano i modelli né sostituiscono S07/S09.
