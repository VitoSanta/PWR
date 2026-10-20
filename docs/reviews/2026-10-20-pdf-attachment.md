# PDF allegato — guida del progetto

Il proprietario segnala un errore ricordato come «symbolich» sul documento
nella cartella Desktop/Documenti/PWR/Progetto. File individuato:
`PWR – Gemma guida.pdf`. L'originale è conservato senza modifiche.

**Aggiornamento finale:** PDF-01 corretto nello sviluppo S02; [prove e limiti](2026-10-20-S02-closeout.md). La riproduzione e il workaround sotto sono storici.

## Riproduzione iniziale

L'esempio CLI `pwr-tools/extract_pdf`, sul sorgente corrente, rifiuta il file
con `ExtractionFailure::Undecodable`. Non serve inferenza per riprodurlo:
`attach_chat_bytes` usa lo stesso estrattore prima di inviare il prompt.
Non è stata osservata qui la stringa esatta segnalata nell'app.

Il file non è un symlink. `pdfinfo` identifica un PDF valido, non cifrato,
di 17 pagine, prodotto da Chromium/Skia. Il comando `file` riportava invece
8 pagine: per il conteggio usare il parser PDF, non quella descrizione sommaria.
Sono presenti font Identity-H e sette riferimenti `/ToUnicode`.
Il `pdftotext -layout` locale recupera 34.108 caratteri con 17 separatori pagina.

Il parser interno legge operatori testuali da stream e interpreta le stringhe
con una codifica semplice WinAnsi/Latin-1. Non risolve font CID e mappe ToUnicode.
Il vecchio rifiuto attribuiva il fallimento a una mappa assente, senza averlo
accertato: è falso per questo documento. Il messaggio ora descrive il limite
dell'estrattore senza accusare il file. La validazione del testo rimane attiva.

## Stato e utilizzo temporaneo

Creata accanto al PDF una copia `PWR – Gemma guida - testo estratto.txt`,
allegabile come testo. È un workaround ottenuto con uno strumento locale,
non un fallback introdotto nel prodotto. Nessuna dipendenza Homebrew o percorso
specifico di questo Mac è stato aggiunto al runtime PWR.

**Il bug di lettura non è risolto dal cambiamento del messaggio.** Occorre
supportare font Unicode con risoluzione delle risorse per pagina e mappe dei
caratteri, mantenendo limiti di decompressione/memoria e compatibilità Windows.
Non rimuovere la verifica di plausibilità né indovinare caratteri dai glyph ID.
Nessuna evidenza richiede OCR per questo documento. È distinto da HI-33
(riconoscimento di `endstream` nel contenuto compresso).

Evidenza privata `pdf-attachment-20261020`: SHA-256 originale, rifiuto prima
della modifica e dati dell'estrazione indipendente. Nove test unitari del parser
riusciti; la lettura dell'allegato nell'app e la consegna al contesto restano
da verificare dopo il supporto effettivo dei font. S00 è stato chiuso con perimetro approvato il 2026-10-20; questo difetto rimane aperto in S01/S02.


## Risoluzione S02

Il parser ora risolve risorse/font per pagina e mappe Unicode con limiti di
espansione. L'originale viene estratto e consegnato interamente al prompt nel
percorso di allegato, senza inferenza e senza modificarne i byte. Regressioni
per mappe, font fra pagine, surrogate pair, bombe e scansioni riuscite.
Il TXT temporaneo non è più necessario per questo documento. PDF-01 è chiuso
nello sviluppo macOS; ordine visivo, OCR e collaudo del pacchetto restano fuori
dalla promessa di questo intervento. Il rapporto finale distingue questi limiti
dal corretto recupero dei caratteri e dalla qualificazione Windows futura.
