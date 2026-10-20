# Studio del sito: difetti trovati e correzioni (2026-10-20/21)

Contesto: cinque modelli Gemma 4 (E2B, E4B, 12B, 26B-A4B, 31B) incaricati di
costruire il sito del progetto da un documento tecnico, con lo stesso prompt,
Goal mode, sandbox, thinking e assistant spenti. Le prime prove nell'app non
producevano un sito che compila. Questa nota raccoglie ciò che si è trovato
leggendo le tracce, le correzioni al core e al motore, e cosa non è provato.

**Perimetro delle prove.** Eseguite dal client headless (`pwr serve`), che non
ha i pannelli di terminale e browser condivisi dell'app: dicono se i modelli
portano a termine il compito, non se usano quei pannelli. Evidenza privata
`site-study-fixes-20261021` nel banco. Nessun gate promosso. Nessun modello
è «qualificato»: una prova è una prova.

**Budget delle prove.** Dalla serie 8 lo stesso per tutti: 150 azioni e 100.000
unità di lavoro (token generati più un ottavo dei token di prompt realmente
letti), con 4 ore solo come rete di sicurezza. Il tempo non è un limite: misura
la macchina, non il modello (il 31B a 8 bit fa un'azione in circa 80 secondi).

## Correzioni

| # | Difetto osservato | Correzione | Dove |
|---|---|---|---|
| 1 | Il `app.html` di partenza (344 righe) non si poteva sostituire con una pagina più corta: 23 rifiuti nel 26B | La guardia sulle riscritture totali non vale per un file ancora identico all'ultimo commit | `pwr-tools` |
| 2 | Nome di programma inventato (`npm_install_check_mock`), 181 volte; «execvp failed» non insegnava nulla | Il messaggio dice che `executable` è solo il programma e dà le chiamate pronte dagli script di `package.json` | `pwr-tools` |
| 3 | Dopo ogni rifiuto «not run again» la cronologia cresceva (6k→45k token) e il modello copiava l'errore | Le coppie di rifiuto ripetute si fondono; il suggerimento originale viene ridetto | `converse.rs` |
| 4 | Il goal non vedeva l'errore finché il modello non chiamava `complete` | Un check-in senza azioni su un progetto modificato esegue i controlli e rimanda l'errore reale | `executor.rs` |
| 5 | `args: "run build"` come stringa contava come risposta inutilizzabile | Parole semplici (senza virgolette né sintassi shell) diventano la lista | `lib.rs` |
| 6 | Il riepilogo diceva «0 di 2 controlli passano» senza l'errore | Il riepilogo include le righe d'errore, senza colori, e dove si trovano | `pwr-cli` |
| 7 | Lo starter non ha `node_modules`; la build «falliva già prima» e il fallimento veniva perdonato | Il preflight dice che mancano le dipendenze; dopo l'installazione l'alibi non vale | `context.rs`, `executor.rs` |
| 8 | Dopo una revisione non applicabile il goal scartava la verifica passata | Chiusura «così com'è», non verificata senza accettazione | `executor.rs` |
| 9–11, 24, 28–29 | Errori comuni senza rimedio (template mancante, import, pacchetto sbagliato, tag sbilanciato, identificatore duplicato, percorso inventato, dipendenze mancanti) | Una frase di rimedio per errore; per import mancanti e percorsi sbagliati, la chiamata `replace_text` completa da copiare quando il file è unico | `pwr-cli` |
| 12 | `complete` scritto come prima riga di testo seguito dal riepilogo veniva letto come prosa | Letto come la chiamata (solo `complete`: il core verifica comunque) | `lib.rs` |
| 13 | `run_command{args:["install"]}` senza `executable`: `install` era l'utilità BSD | Verbo npm senza programma, in un progetto con `package.json`, completato con `npm` | `pwr-tools` |
| 14 | Un ciclo fermava il goal senza rimostrare l'errore | Rimandato indietro con l'errore, al massimo due volte | `executor.rs` |
| 15 | Tre risposte inutilizzabili chiudevano il goal (colpo di campionamento) | Fino a tre nuovi tentativi per goal | `executor.rs` |
| 16–17 | Nessun file scritto e solo domande; oppure lavoro fatto, controlli verdi e silenzio | Richiamo a scrivere un file; il silenzio dopo lavoro verde vale come chiusura (con verifica e revisione) | `executor.rs` |
| 18 | E4B: 9 richieste su 44 con la cache del prompt buttata via (48–51 s ciascuna a 49k token) | Con thinking spento il ragionamento non viene rimandato nella cronologia: il rendering resta stabile | `pwr_mlx.py` |
| 19–23 | Errore senza file; test starter rosso; richiami che esaurivano le verifiche | Posizione dell'errore; rimedio per lo spec; le letture dei richiami hanno un tetto proprio (40) | `pwr-cli`, `executor.rs` |
| 25–27 | Ciclo con controlli verdi chiuso come `recovery`; troppo pochi check-in; `args:["npm install"]` senza `executable` | Chiusura «così com'è»; 5 check-in; riga completa letta come chiamata | `executor.rs`, `lib.rs` |
| 20 | Il modello lavora senza chiudere (79 riscritture della stessa pagina) | Ogni 30 azioni il goal legge i controlli: rossi → l'errore, verdi → «puoi chiudere con `complete`» | `executor.rs` |
| 30 | Server locale dei test: una sola `read` della richiesta, reset di connessione in 1 esecuzione su 3 | Legge fino alla fine delle intestazioni; test con richiesta in due pezzi | `pwr-tools/site.rs` |

Nessuna correzione tocca `workspace_interact`, `look_at`, il terminale condiviso
né il browser condiviso; i loro test restano verdi.

## Esiti (build e test rifatti nella cartella di ogni prova)

Conteggio delle prove dalla serie 5 in poi (budget a giri e lavoro, correzioni
via via accumulate), per «la cartella finale compila». Una prova che si ferma
dopo poche azioni sul progetto di partenza non conta come prova di lavoro.

| Modello | Cartella finale compila | Note |
|---|---|---|
| 31B (8 bit) | 2 su 2 | una chiusa per intero: `complete`, build e test verdi, 41 azioni, revisione finale 945 s su 4.059; l'altra verde ma fermata dal tempo di prova |
| 26B-A4B | 3 su 4 | due con build e test verdi (una chiusa dal budget a forza di rifiniture, prima del fix 20; una come `recovery` per il ciclo, prima del fix 25); una rossa (`TS2339`) |
| 12B | 2 su 4 | una chiusa bene in 20 azioni; due rosse su import e tag |
| E4B | 3 su 6 | il test starter resta spesso rosso; la cache instabile lo rendeva 6 volte più lento (fix 18) |
| E2B | 1 su 7 | l'unico che ancora si ferma presto: percorsi e import inventati |

Numeri piccoli, una prova per cella e correzioni cambiate tra una serie e
l'altra: **non è una misura di affidabilità**. La variabilità tra prove con lo
stesso modello è grande (26B: 89 azioni a build verde in una prova, 8 azioni
e tre risposte inutilizzabili in un'altra).

## Limiti e decisioni aperte

- Le prove non esercitano il browser e il terminale condivisi: da verificare
  nell'app con lo stesso prompt.
- Il tetto di tempo del goal (60 minuti) è ancora il solo limite nel prodotto
  quando `work` non è impostato; il 31B vi arriverebbe a circa 45 azioni.
  Proposta: giri e lavoro come limiti, tempo come sicurezza. Cambia un default,
  **non applicato**.
- Il richiamo ogni 30 azioni e la chiusura dopo un silenzio sono scelte di
  prodotto, attive nel goal: da confermare dopo le prove nell'app.
- La revisione finale del 31B è durata 945 s: nessun tetto alla revisione è
  stato imposto.
- Le correzioni pronte (`replace_text` completo) sono copiabili, non applicate
  dal core: il modello fa la chiamata con i propri strumenti.
