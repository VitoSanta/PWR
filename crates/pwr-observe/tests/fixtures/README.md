# Log di run veri, come dati di test

Tre run Angular del 7 settembre 2026, esportati dal loro `state.sqlite`. Non
sono costruiti: sono ciò che è successo, e ogni rilevatore in `diagnose.rs`
esiste perché una di queste tracce è stata letta a mano una volta.

| Fixture | Run | Come è finito |
|---|---|---|
| `angular-ornith-before-batching` | ornith-1.5:35b, binario senza batch di letture | stallo, zero componenti |
| `angular-ornith-first-batching` | ornith-1.5:35b, prima versione del batch | tre turni senza risposta col prompt al 97% del contesto |
| `angular-gptoss` | gpt-oss:20b | stallo con la build rotta e quattro avvisi ignorati |

Le stringhe oltre i 400 caratteri e gli elenchi oltre gli otto elementi sono
sostituiti dalla loro dimensione: nessun rilevatore li legge, e una fixture che
portasse un CV, tre componenti e ogni listato di cartella sarebbe un corpus
committato per sbaglio. Tutto il resto — tipi di evento, capability, percorsi,
stati, contatori del backend — è intatto.
