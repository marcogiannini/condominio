//! Database SQLite: schema e accesso.

use anyhow::{Context, Result};
use rusqlite::Connection;
use std::path::{Path, PathBuf};

pub struct Db {
    pub conn: Connection,
    /// Percorso del file database (None per i test in memoria).
    pub percorso: Option<PathBuf>,
    /// Cartella in cui vengono copiati i documenti archiviati.
    pub archivio_dir: PathBuf,
}

const SCHEMA_V1: &str = r#"
CREATE TABLE condominio(
  id INTEGER PRIMARY KEY, nome TEXT NOT NULL, indirizzo TEXT NOT NULL DEFAULT '',
  citta TEXT NOT NULL DEFAULT '', codice_fiscale TEXT NOT NULL DEFAULT '', note TEXT NOT NULL DEFAULT ''
);

CREATE TABLE persona(
  id INTEGER PRIMARY KEY,
  tipo TEXT NOT NULL DEFAULT 'fisica' CHECK(tipo IN ('fisica','giuridica')),
  nome TEXT NOT NULL DEFAULT '', cognome TEXT NOT NULL,
  codice_fiscale TEXT NOT NULL DEFAULT '', email TEXT NOT NULL DEFAULT '',
  telefono TEXT NOT NULL DEFAULT '', indirizzo TEXT NOT NULL DEFAULT '', note TEXT NOT NULL DEFAULT ''
);

CREATE TABLE unita(
  id INTEGER PRIMARY KEY,
  condominio_id INTEGER NOT NULL REFERENCES condominio(id) ON DELETE CASCADE,
  scala TEXT NOT NULL DEFAULT '', piano TEXT NOT NULL DEFAULT '', interno TEXT NOT NULL DEFAULT '',
  tipo TEXT NOT NULL DEFAULT 'appartamento', dati_catastali TEXT NOT NULL DEFAULT '', note TEXT NOT NULL DEFAULT ''
);

-- Titolarità: più soggetti per unità (multi-proprietario). quota_ppm: 1_000_000 = 100%.
CREATE TABLE titolarita(
  id INTEGER PRIMARY KEY,
  unita_id INTEGER NOT NULL REFERENCES unita(id) ON DELETE CASCADE,
  persona_id INTEGER NOT NULL REFERENCES persona(id),
  ruolo TEXT NOT NULL CHECK(ruolo IN ('proprietario','inquilino','usufruttuario')),
  quota_ppm INTEGER NOT NULL DEFAULT 1000000 CHECK(quota_ppm > 0 AND quota_ppm <= 1000000),
  paga_spese INTEGER NOT NULL DEFAULT 0,
  dal TEXT, al TEXT
);

CREATE TABLE conto_contabile(
  id INTEGER PRIMARY KEY,
  condominio_id INTEGER NOT NULL REFERENCES condominio(id) ON DELETE CASCADE,
  codice TEXT NOT NULL, nome TEXT NOT NULL,
  tipo TEXT NOT NULL CHECK(tipo IN ('attivo','passivo','costo','ricavo','patrimonio')),
  UNIQUE(condominio_id, codice)
);

CREATE TABLE conto_corrente(
  id INTEGER PRIMARY KEY,
  condominio_id INTEGER NOT NULL REFERENCES condominio(id) ON DELETE CASCADE,
  nome TEXT NOT NULL, iban TEXT NOT NULL DEFAULT '',
  conto_contabile_id INTEGER NOT NULL REFERENCES conto_contabile(id)
);

CREATE TABLE tabella_millesimale(
  id INTEGER PRIMARY KEY,
  condominio_id INTEGER NOT NULL REFERENCES condominio(id) ON DELETE CASCADE,
  codice TEXT NOT NULL, nome TEXT NOT NULL, descrizione TEXT NOT NULL DEFAULT '',
  totale_micro INTEGER NOT NULL DEFAULT 1000000,
  UNIQUE(condominio_id, codice)
);

CREATE TABLE millesimo(
  tabella_id INTEGER NOT NULL REFERENCES tabella_millesimale(id) ON DELETE CASCADE,
  unita_id INTEGER NOT NULL REFERENCES unita(id) ON DELETE CASCADE,
  valore_micro INTEGER NOT NULL CHECK(valore_micro >= 0),
  PRIMARY KEY(tabella_id, unita_id)
);

CREATE TABLE esercizio(
  id INTEGER PRIMARY KEY,
  condominio_id INTEGER NOT NULL REFERENCES condominio(id) ON DELETE CASCADE,
  nome TEXT NOT NULL, dal TEXT NOT NULL, al TEXT NOT NULL,
  stato TEXT NOT NULL DEFAULT 'aperto' CHECK(stato IN ('aperto','chiuso'))
);

CREATE TABLE delibera(
  id INTEGER PRIMARY KEY,
  condominio_id INTEGER NOT NULL REFERENCES condominio(id) ON DELETE CASCADE,
  esercizio_id INTEGER REFERENCES esercizio(id),
  data TEXT NOT NULL,
  tipo TEXT NOT NULL DEFAULT 'altro' CHECK(tipo IN ('preventivo','rendiconto','straordinaria','integrativa','altro')),
  oggetto TEXT NOT NULL,
  riferimento TEXT NOT NULL DEFAULT '',   -- es. "Verbale assemblea del 12/03, punto 4"
  maggioranza TEXT NOT NULL DEFAULT ''    -- es. "Art. 1136 c.2 c.c."
);

-- Piano dei conti di spesa (preventivo), gerarchico: capitoli -> voci.
CREATE TABLE voce_spesa(
  id INTEGER PRIMARY KEY,
  esercizio_id INTEGER NOT NULL REFERENCES esercizio(id) ON DELETE CASCADE,
  parent_id INTEGER REFERENCES voce_spesa(id),
  codice TEXT NOT NULL, descrizione TEXT NOT NULL,
  importo_cents INTEGER NOT NULL DEFAULT 0,
  tabella_id INTEGER REFERENCES tabella_millesimale(id),
  sopravvenienza INTEGER NOT NULL DEFAULT 0   -- Art. 1130-bis c.c.
);

CREATE TABLE piano_rate(
  id INTEGER PRIMARY KEY,
  esercizio_id INTEGER NOT NULL REFERENCES esercizio(id) ON DELETE CASCADE,
  nome TEXT NOT NULL,
  tipo TEXT NOT NULL CHECK(tipo IN ('ordinario','straordinario','integrativo')),
  stato TEXT NOT NULL DEFAULT 'bozza' CHECK(stato IN ('bozza','attivo','annullato')),
  delibera_id INTEGER REFERENCES delibera(id),
  n_rate INTEGER NOT NULL DEFAULT 1 CHECK(n_rate >= 1),
  prima_scadenza TEXT NOT NULL, cadenza_mesi INTEGER NOT NULL DEFAULT 1 CHECK(cadenza_mesi >= 1),
  registrazione_id INTEGER, creato_il TEXT NOT NULL
);

CREATE TABLE piano_voce(
  piano_id INTEGER NOT NULL REFERENCES piano_rate(id) ON DELETE CASCADE,
  voce_id INTEGER NOT NULL REFERENCES voce_spesa(id),
  importo_cents INTEGER NOT NULL CHECK(importo_cents >= 0),
  PRIMARY KEY(piano_id, voce_id)
);

CREATE TABLE rata(
  id INTEGER PRIMARY KEY,
  piano_id INTEGER NOT NULL REFERENCES piano_rate(id) ON DELETE CASCADE,
  unita_id INTEGER NOT NULL REFERENCES unita(id),
  persona_id INTEGER NOT NULL REFERENCES persona(id),
  numero INTEGER NOT NULL, scadenza TEXT NOT NULL,
  importo_cents INTEGER NOT NULL,
  interruzione_prescrizione TEXT
);
CREATE INDEX idx_rata_persona ON rata(persona_id, scadenza);

CREATE TABLE incasso(
  id INTEGER PRIMARY KEY,
  condominio_id INTEGER NOT NULL REFERENCES condominio(id) ON DELETE CASCADE,
  persona_id INTEGER NOT NULL REFERENCES persona(id),
  conto_corrente_id INTEGER REFERENCES conto_corrente(id),
  data TEXT NOT NULL,
  importo_cents INTEGER NOT NULL DEFAULT 0,          -- versato (banca/cassa)
  da_salvadanaio_cents INTEGER NOT NULL DEFAULT 0,   -- credito pregresso compensato
  eccedenza_cents INTEGER NOT NULL DEFAULT 0,        -- versato oltre il dovuto -> Salvadanaio
  metodo TEXT NOT NULL DEFAULT 'bonifico', note TEXT NOT NULL DEFAULT '',
  stato TEXT NOT NULL DEFAULT 'valido' CHECK(stato IN ('valido','stornato')),
  registrazione_id INTEGER, storno_registrazione_id INTEGER, data_storno TEXT
);

CREATE TABLE allocazione(
  id INTEGER PRIMARY KEY,
  incasso_id INTEGER NOT NULL REFERENCES incasso(id) ON DELETE CASCADE,
  rata_id INTEGER NOT NULL REFERENCES rata(id),
  importo_cents INTEGER NOT NULL CHECK(importo_cents > 0)
);
CREATE INDEX idx_alloc_rata ON allocazione(rata_id);

-- Salvadanaio (Smart Wallet): registro movimenti, saldo = somma.
CREATE TABLE wallet_mov(
  id INTEGER PRIMARY KEY,
  condominio_id INTEGER NOT NULL REFERENCES condominio(id) ON DELETE CASCADE,
  persona_id INTEGER NOT NULL REFERENCES persona(id),
  data TEXT NOT NULL, importo_cents INTEGER NOT NULL, causale TEXT NOT NULL,
  incasso_id INTEGER REFERENCES incasso(id)
);

CREATE TABLE registrazione(
  id INTEGER PRIMARY KEY,
  condominio_id INTEGER NOT NULL REFERENCES condominio(id) ON DELETE CASCADE,
  data TEXT NOT NULL, descrizione TEXT NOT NULL, protocollo INTEGER NOT NULL,
  origine TEXT NOT NULL DEFAULT 'manuale', origine_id INTEGER,
  storno_di INTEGER REFERENCES registrazione(id)
);

CREATE TABLE riga_reg(
  id INTEGER PRIMARY KEY,
  registrazione_id INTEGER NOT NULL REFERENCES registrazione(id) ON DELETE CASCADE,
  conto_id INTEGER NOT NULL REFERENCES conto_contabile(id),
  dare_cents INTEGER NOT NULL DEFAULT 0 CHECK(dare_cents >= 0),
  avere_cents INTEGER NOT NULL DEFAULT 0 CHECK(avere_cents >= 0)
);

CREATE TABLE fornitore(
  id INTEGER PRIMARY KEY,
  condominio_id INTEGER NOT NULL REFERENCES condominio(id) ON DELETE CASCADE,
  ragione_sociale TEXT NOT NULL, cf_piva TEXT NOT NULL DEFAULT '',
  email TEXT NOT NULL DEFAULT '', iban TEXT NOT NULL DEFAULT '',
  regime_ritenuta TEXT NOT NULL DEFAULT 'nessuna' CHECK(regime_ritenuta IN ('nessuna','appalti4','professionista20')),
  soggetto_ires INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE versamento_ritenute(
  id INTEGER PRIMARY KEY,
  condominio_id INTEGER NOT NULL REFERENCES condominio(id) ON DELETE CASCADE,
  data TEXT NOT NULL, codice_tributo TEXT NOT NULL, importo_cents INTEGER NOT NULL,
  conto_corrente_id INTEGER REFERENCES conto_corrente(id), registrazione_id INTEGER
);

CREATE TABLE fattura(
  id INTEGER PRIMARY KEY,
  condominio_id INTEGER NOT NULL REFERENCES condominio(id) ON DELETE CASCADE,
  esercizio_id INTEGER NOT NULL REFERENCES esercizio(id),
  fornitore_id INTEGER NOT NULL REFERENCES fornitore(id),
  voce_id INTEGER REFERENCES voce_spesa(id),
  numero TEXT NOT NULL, data TEXT NOT NULL, descrizione TEXT NOT NULL DEFAULT '',
  imponibile_cents INTEGER NOT NULL, iva_cents INTEGER NOT NULL DEFAULT 0, ritenuta_cents INTEGER NOT NULL DEFAULT 0,
  stato TEXT NOT NULL DEFAULT 'da_pagare' CHECK(stato IN ('da_pagare','pagata','stornata')),
  data_pagamento TEXT, conto_corrente_id INTEGER REFERENCES conto_corrente(id),
  registrazione_id INTEGER, registrazione_pag_id INTEGER,
  versamento_id INTEGER REFERENCES versamento_ritenute(id)
);

CREATE TABLE documento(
  id INTEGER PRIMARY KEY,
  condominio_id INTEGER NOT NULL REFERENCES condominio(id) ON DELETE CASCADE,
  tipo TEXT NOT NULL DEFAULT 'altro', titolo TEXT NOT NULL, descrizione TEXT NOT NULL DEFAULT '',
  nome_file TEXT NOT NULL, percorso TEXT NOT NULL, data TEXT NOT NULL,
  dimensione INTEGER NOT NULL DEFAULT 0, creato_il TEXT NOT NULL
);
CREATE VIRTUAL TABLE documento_fts USING fts5(titolo, descrizione, contenuto, tokenize='unicode61 remove_diacritics 2');

CREATE TABLE ticket(
  id INTEGER PRIMARY KEY,
  condominio_id INTEGER NOT NULL REFERENCES condominio(id) ON DELETE CASCADE,
  unita_id INTEGER REFERENCES unita(id), persona_id INTEGER REFERENCES persona(id),
  titolo TEXT NOT NULL, descrizione TEXT NOT NULL DEFAULT '',
  priorita INTEGER NOT NULL DEFAULT 1 CHECK(priorita BETWEEN 0 AND 3),
  stato TEXT NOT NULL DEFAULT 'aperta' CHECK(stato IN ('aperta','in_carico','in_attesa','risolta','chiusa')),
  creato_il TEXT NOT NULL, aggiornato_il TEXT NOT NULL
);

CREATE TABLE ticket_commento(
  id INTEGER PRIMARY KEY,
  ticket_id INTEGER NOT NULL REFERENCES ticket(id) ON DELETE CASCADE,
  autore TEXT NOT NULL, testo TEXT NOT NULL, creato_il TEXT NOT NULL
);

CREATE TABLE notifica(
  id INTEGER PRIMARY KEY,
  condominio_id INTEGER REFERENCES condominio(id) ON DELETE CASCADE,
  creato_il TEXT NOT NULL, tipo TEXT NOT NULL, messaggio TEXT NOT NULL, letta INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE comunicazione(
  id INTEGER PRIMARY KEY,
  condominio_id INTEGER NOT NULL REFERENCES condominio(id) ON DELETE CASCADE,
  tipo TEXT NOT NULL DEFAULT 'avviso' CHECK(tipo IN ('avviso','assemblea','circolare')),
  titolo TEXT NOT NULL, testo TEXT NOT NULL DEFAULT '',
  data_evento TEXT, creato_il TEXT NOT NULL, pubblicata INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE email_log(
  id INTEGER PRIMARY KEY,
  comunicazione_id INTEGER REFERENCES comunicazione(id) ON DELETE SET NULL,
  destinatario TEXT NOT NULL, oggetto TEXT NOT NULL, corpo TEXT NOT NULL,
  stato TEXT NOT NULL DEFAULT 'in_coda' CHECK(stato IN ('in_coda','inviata','errore')),
  errore TEXT NOT NULL DEFAULT '', creato_il TEXT NOT NULL, inviato_il TEXT
);

CREATE TABLE impostazione(chiave TEXT PRIMARY KEY, valore TEXT NOT NULL);
"#;


const SCHEMA_V2: &str = r#"
-- Tasso di interesse legale per anno (ppm: 1_000_000 = 100%; 1,6% = 16000). Modificabile dall'utente.
CREATE TABLE tasso_legale(anno INTEGER PRIMARY KEY, ppm INTEGER NOT NULL CHECK(ppm >= 0));
INSERT INTO tasso_legale(anno,ppm) VALUES
 (2019,8000),(2020,500),(2021,100),(2022,12500),(2023,50000),(2024,25000),(2025,20000),(2026,16000);

-- Estratto conto bancario importato (riconciliazione).
CREATE TABLE movimento_banca(
  id INTEGER PRIMARY KEY,
  condominio_id INTEGER NOT NULL REFERENCES condominio(id) ON DELETE CASCADE,
  conto_corrente_id INTEGER NOT NULL REFERENCES conto_corrente(id) ON DELETE CASCADE,
  data TEXT NOT NULL, descrizione TEXT NOT NULL DEFAULT '', importo_cents INTEGER NOT NULL,
  impronta TEXT NOT NULL,
  stato TEXT NOT NULL DEFAULT 'da_abbinare' CHECK(stato IN ('da_abbinare','riconciliato','ignorato')),
  incasso_id INTEGER REFERENCES incasso(id), fattura_id INTEGER REFERENCES fattura(id),
  versamento_id INTEGER REFERENCES versamento_ritenute(id),
  UNIQUE(conto_corrente_id, impronta)
);

CREATE TABLE agenda(
  id INTEGER PRIMARY KEY,
  condominio_id INTEGER NOT NULL REFERENCES condominio(id) ON DELETE CASCADE,
  data TEXT NOT NULL, titolo TEXT NOT NULL, note TEXT NOT NULL DEFAULT '',
  completata INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE assemblea(
  id INTEGER PRIMARY KEY,
  condominio_id INTEGER NOT NULL REFERENCES condominio(id) ON DELETE CASCADE,
  esercizio_id INTEGER REFERENCES esercizio(id),
  tipo TEXT NOT NULL DEFAULT 'ordinaria' CHECK(tipo IN ('ordinaria','straordinaria')),
  stato TEXT NOT NULL DEFAULT 'bozza' CHECK(stato IN ('bozza','convocata','tenuta','verbalizzata')),
  modalita TEXT NOT NULL DEFAULT 'presenza' CHECK(modalita IN ('presenza','online','mista')),
  luogo TEXT NOT NULL DEFAULT '',
  data_prima TEXT NOT NULL, ora_prima TEXT NOT NULL DEFAULT '',
  data_seconda TEXT, ora_seconda TEXT NOT NULL DEFAULT '',
  tabella_id INTEGER REFERENCES tabella_millesimale(id),   -- tabella "proprietà generale" per i quorum
  convocazione_tenuta INTEGER CHECK(convocazione_tenuta IN (1,2)),
  presidente TEXT NOT NULL DEFAULT '', segretario TEXT NOT NULL DEFAULT '',
  comunicazione_id INTEGER REFERENCES comunicazione(id) ON DELETE SET NULL,
  creato_il TEXT NOT NULL
);

CREATE TABLE assemblea_punto(
  id INTEGER PRIMARY KEY,
  assemblea_id INTEGER NOT NULL REFERENCES assemblea(id) ON DELETE CASCADE,
  numero INTEGER NOT NULL, titolo TEXT NOT NULL, descrizione TEXT NOT NULL DEFAULT '',
  maggioranza TEXT NOT NULL DEFAULT 'ordinaria'
    CHECK(maggioranza IN ('ordinaria','qualificata','innovazioni','unanimita')),
  tipo_delibera TEXT NOT NULL DEFAULT 'altro',
  delibera_id INTEGER REFERENCES delibera(id)
);

CREATE TABLE assemblea_presenza(
  assemblea_id INTEGER NOT NULL REFERENCES assemblea(id) ON DELETE CASCADE,
  unita_id INTEGER NOT NULL REFERENCES unita(id) ON DELETE CASCADE,
  rappresentante TEXT NOT NULL DEFAULT '',    -- vuoto = interviene il titolare; altrimenti nome del delegato
  PRIMARY KEY(assemblea_id, unita_id)
);

CREATE TABLE assemblea_voto(
  punto_id INTEGER NOT NULL REFERENCES assemblea_punto(id) ON DELETE CASCADE,
  unita_id INTEGER NOT NULL REFERENCES unita(id) ON DELETE CASCADE,
  scelta TEXT NOT NULL CHECK(scelta IN ('favorevole','contrario','astenuto')),
  PRIMARY KEY(punto_id, unita_id)
);

CREATE TABLE pratica(
  id INTEGER PRIMARY KEY,
  condominio_id INTEGER NOT NULL REFERENCES condominio(id) ON DELETE CASCADE,
  tipo TEXT NOT NULL CHECK(tipo IN ('intervento','sinistro','legale')),
  titolo TEXT NOT NULL, descrizione TEXT NOT NULL DEFAULT '',
  stato TEXT NOT NULL,
  fornitore_id INTEGER REFERENCES fornitore(id) ON DELETE SET NULL,
  unita_id INTEGER REFERENCES unita(id) ON DELETE SET NULL,
  ticket_id INTEGER REFERENCES ticket(id) ON DELETE SET NULL,
  riferimento TEXT NOT NULL DEFAULT '',   -- n. sinistro / polizza / RG
  controparte TEXT NOT NULL DEFAULT '',
  importo_cents INTEGER NOT NULL DEFAULT 0,
  data_apertura TEXT NOT NULL, data_chiusura TEXT, creato_il TEXT NOT NULL
);

CREATE TABLE pratica_evento(
  id INTEGER PRIMARY KEY,
  pratica_id INTEGER NOT NULL REFERENCES pratica(id) ON DELETE CASCADE,
  data TEXT NOT NULL, testo TEXT NOT NULL
);

-- Solleciti inviati ai condòmini morosi (storico).
CREATE TABLE sollecito(
  id INTEGER PRIMARY KEY,
  condominio_id INTEGER NOT NULL REFERENCES condominio(id) ON DELETE CASCADE,
  persona_id INTEGER NOT NULL REFERENCES persona(id),
  data TEXT NOT NULL, canale TEXT NOT NULL DEFAULT 'email',
  capitale_cents INTEGER NOT NULL, interessi_cents INTEGER NOT NULL,
  testo TEXT NOT NULL DEFAULT ''
);

-- Conguaglio di fine esercizio (consuntivo approvato): una sola emissione per esercizio, annullabile.
CREATE TABLE conguaglio(
  esercizio_id INTEGER PRIMARY KEY REFERENCES esercizio(id) ON DELETE CASCADE,
  data TEXT NOT NULL,
  delibera_id INTEGER NOT NULL REFERENCES delibera(id),
  piano_id INTEGER REFERENCES piano_rate(id),
  registrazione_credito_id INTEGER
);
CREATE TABLE conguaglio_credito(
  esercizio_id INTEGER NOT NULL REFERENCES conguaglio(esercizio_id) ON DELETE CASCADE,
  persona_id INTEGER NOT NULL REFERENCES persona(id),
  importo_cents INTEGER NOT NULL CHECK(importo_cents > 0),
  PRIMARY KEY(esercizio_id, persona_id)
);
"#;

/// Schema v3: modello "Domustudio" (movimenti di gestione) al posto della partita doppia, riparti su più tabelle.
/// Le scritture contabili esistenti vengono convertite nei movimenti dei conti correnti (nessun dato bancario si perde).
const SCHEMA_V3: &str = r#"
CREATE TABLE movimento_gestione(
  id INTEGER PRIMARY KEY,
  condominio_id INTEGER NOT NULL REFERENCES condominio(id) ON DELETE CASCADE,
  conto_corrente_id INTEGER NOT NULL REFERENCES conto_corrente(id) ON DELETE CASCADE,
  data TEXT NOT NULL, descrizione TEXT NOT NULL DEFAULT '',
  importo_cents INTEGER NOT NULL CHECK(importo_cents <> 0),   -- + entrata, - uscita
  origine TEXT NOT NULL DEFAULT 'manuale',
  origine_id INTEGER,
  voce_id INTEGER REFERENCES voce_spesa(id) ON DELETE SET NULL,   -- solo movimenti manuali: imputazione al consuntivo
  storno_di INTEGER REFERENCES movimento_gestione(id),            -- il movimento che questo storna
  stornato INTEGER NOT NULL DEFAULT 0,
  reg_src INTEGER                                                 -- temporaneo (migrazione)
);
CREATE INDEX idx_mg_cc ON movimento_gestione(conto_corrente_id, data);
CREATE INDEX idx_mg_cond ON movimento_gestione(condominio_id, data);

INSERT INTO movimento_gestione(condominio_id,conto_corrente_id,data,descrizione,importo_cents,origine,origine_id,reg_src)
  SELECT g.condominio_id, cc.id, g.data, g.descrizione, rr.dare_cents-rr.avere_cents, g.origine, g.origine_id, g.id
  FROM riga_reg rr
  JOIN registrazione g ON g.id=rr.registrazione_id
  JOIN conto_corrente cc ON cc.conto_contabile_id=rr.conto_id
  WHERE rr.dare_cents<>rr.avere_cents
  ORDER BY g.id, rr.id;

UPDATE movimento_gestione SET storno_di=(
    SELECT o.id FROM movimento_gestione o
    WHERE o.reg_src=movimento_gestione.origine_id AND o.conto_corrente_id=movimento_gestione.conto_corrente_id LIMIT 1)
  WHERE origine='storno';
UPDATE movimento_gestione SET stornato=1 WHERE id IN (SELECT storno_di FROM movimento_gestione WHERE storno_di IS NOT NULL);
UPDATE movimento_gestione SET origine_id=storno_di WHERE origine='storno';

ALTER TABLE incasso RENAME COLUMN registrazione_id TO movimento_id;
ALTER TABLE incasso RENAME COLUMN storno_registrazione_id TO storno_movimento_id;
UPDATE incasso SET movimento_id=(SELECT m.id FROM movimento_gestione m WHERE m.reg_src=incasso.movimento_id AND m.origine='incasso' LIMIT 1);
UPDATE incasso SET storno_movimento_id=(SELECT m.id FROM movimento_gestione m WHERE m.reg_src=incasso.storno_movimento_id LIMIT 1);

ALTER TABLE fattura RENAME COLUMN registrazione_pag_id TO movimento_pag_id;
UPDATE fattura SET movimento_pag_id=(SELECT m.id FROM movimento_gestione m WHERE m.reg_src=fattura.movimento_pag_id LIMIT 1);
ALTER TABLE fattura DROP COLUMN registrazione_id;

ALTER TABLE versamento_ritenute RENAME COLUMN registrazione_id TO movimento_id;
UPDATE versamento_ritenute SET movimento_id=(SELECT m.id FROM movimento_gestione m WHERE m.reg_src=versamento_ritenute.movimento_id LIMIT 1);

ALTER TABLE piano_rate DROP COLUMN registrazione_id;
ALTER TABLE conguaglio DROP COLUMN registrazione_credito_id;
ALTER TABLE movimento_gestione DROP COLUMN reg_src;

DROP TABLE riga_reg;
DROP TABLE registrazione;

CREATE TABLE conto_corrente_nuovo(
  id INTEGER PRIMARY KEY,
  condominio_id INTEGER NOT NULL REFERENCES condominio(id) ON DELETE CASCADE,
  nome TEXT NOT NULL, iban TEXT NOT NULL DEFAULT ''
);
INSERT INTO conto_corrente_nuovo(id,condominio_id,nome,iban) SELECT id,condominio_id,nome,iban FROM conto_corrente;
DROP TABLE conto_corrente;
ALTER TABLE conto_corrente_nuovo RENAME TO conto_corrente;
DROP TABLE conto_contabile;

-- Riparto di una voce di spesa su più tabelle (quota_ppm: 1_000_000 = 100%). Senza righe vale voce_spesa.tabella_id.
CREATE TABLE voce_tabella(
  voce_id INTEGER NOT NULL REFERENCES voce_spesa(id) ON DELETE CASCADE,
  tabella_id INTEGER NOT NULL REFERENCES tabella_millesimale(id),
  quota_ppm INTEGER NOT NULL CHECK(quota_ppm > 0 AND quota_ppm <= 1000000),
  PRIMARY KEY(voce_id, tabella_id)
);

-- Riduzioni/esclusioni per unità su una voce (riduzione_ppm: 1_000_000 = unità esclusa dalla spesa).
CREATE TABLE voce_riduzione(
  voce_id INTEGER NOT NULL REFERENCES voce_spesa(id) ON DELETE CASCADE,
  unita_id INTEGER NOT NULL REFERENCES unita(id) ON DELETE CASCADE,
  riduzione_ppm INTEGER NOT NULL CHECK(riduzione_ppm > 0 AND riduzione_ppm <= 1000000),
  motivo TEXT NOT NULL DEFAULT '',
  PRIMARY KEY(voce_id, unita_id)
);
"#;

pub const VERSIONE_SCHEMA: i64 = 3;

impl Db {
    pub fn apri(percorso: &Path, archivio_dir: &Path) -> Result<Db> {
        if let Some(p) = percorso.parent() {
            std::fs::create_dir_all(p).ok();
        }
        std::fs::create_dir_all(archivio_dir).ok();
        let conn = Connection::open(percorso).with_context(|| format!("apertura database {}", percorso.display()))?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
        let db = Db { conn, percorso: Some(percorso.to_path_buf()), archivio_dir: archivio_dir.to_path_buf() };
        db.migra()?;
        Ok(db)
    }

    pub fn in_memoria() -> Result<Db> {
        let conn = Connection::open_in_memory()?;
        let dir = std::env::temp_dir().join(format!("condominio-test-{}", std::process::id()));
        let db = Db { conn, percorso: None, archivio_dir: dir };
        db.migra()?;
        Ok(db)
    }

    /// Database in memoria fermo a una versione intermedia dello schema (test di migrazione).
    pub fn in_memoria_a(versione: i64) -> Result<Db> {
        let conn = Connection::open_in_memory()?;
        let dir = std::env::temp_dir().join(format!("condominio-test-{}", std::process::id()));
        let db = Db { conn, percorso: None, archivio_dir: dir };
        db.migra_a(versione)?;
        Ok(db)
    }

    pub fn migra(&self) -> Result<()> {
        self.migra_a(VERSIONE_SCHEMA)
    }

    /// Porta lo schema fino alla versione `massima` (usato dai test di migrazione).
    pub fn migra_a(&self, massima: i64) -> Result<()> {
        self.conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        let v: i64 = self.conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        for (versione, sql) in [(1, SCHEMA_V1), (2, SCHEMA_V2), (3, SCHEMA_V3)] {
            if v >= versione || versione > massima {
                continue;
            }
            // la v3 ricostruisce tabelle: le chiavi esterne vanno sospese (e verificate prima di confermare)
            self.conn.execute_batch("PRAGMA foreign_keys=OFF;")?;
            self.conn.execute_batch("BEGIN;")?;
            let r = self
                .conn
                .execute_batch(sql)
                .map_err(anyhow::Error::from)
                .and_then(|_| {
                    let violazioni: i64 = self.conn.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| r.get(0))?;
                    if violazioni > 0 {
                        anyhow::bail!("migrazione schema v{}: {} violazioni di chiavi esterne", versione, violazioni);
                    }
                    Ok(())
                })
                .and_then(|_| self.conn.execute_batch(&format!("PRAGMA user_version={};", versione)).map_err(Into::into));
            let esito = match r {
                Ok(_) => self.conn.execute_batch("COMMIT;").map_err(anyhow::Error::from),
                Err(e) => {
                    let _ = self.conn.execute_batch("ROLLBACK;");
                    Err(e)
                }
            };
            self.conn.execute_batch("PRAGMA foreign_keys=ON;")?;
            esito?;
        }
        Ok(())
    }

    pub fn atomico<T>(&self, f: impl FnOnce(&Db) -> Result<T>) -> Result<T> {
        self.conn.execute_batch("SAVEPOINT atomico")?;
        match f(self) {
            Ok(v) => {
                self.conn.execute_batch("RELEASE atomico")?;
                Ok(v)
            }
            Err(e) => {
                let _ = self.conn.execute_batch("ROLLBACK TO atomico; RELEASE atomico");
                Err(e)
            }
        }
    }

    pub fn imposta(&self, chiave: &str, valore: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO impostazione(chiave,valore) VALUES(?1,?2) ON CONFLICT(chiave) DO UPDATE SET valore=?2",
            rusqlite::params![chiave, valore],
        )?;
        Ok(())
    }

    pub fn leggi_imp(&self, chiave: &str) -> String {
        self.conn
            .query_row("SELECT valore FROM impostazione WHERE chiave=?1", [chiave], |r| r.get::<_, String>(0))
            .unwrap_or_default()
    }

    /// Copia di backup consistente del database.
    pub fn backup(&self, destinazione: &Path) -> Result<()> {
        if destinazione.exists() {
            std::fs::remove_file(destinazione)?;
        }
        let d = destinazione.to_string_lossy().replace('\'', "''");
        self.conn.execute_batch(&format!("VACUUM INTO '{}'", d))?;
        Ok(())
    }
}
