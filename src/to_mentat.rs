
use failure;
use std::fs::{self, File};
use std::io::{Read, Write, self};
use std::fmt::{Write as FmtWrite};
use std::path::PathBuf;
use std::collections::HashMap;
use tempfile;
use rand::prelude::*;
use util::humanize_size;

use rusqlite::{
    Connection,
    OpenFlags,
    Row,
};

use mentat::{
    self,
    Store,
    Keyword,
    Queryable,
    errors::Result as MentatResult,
};

#[derive(Debug, Clone)]
struct TransactBuilder {
    counter: u64,
    data: String,
    total_terms: u64,
    terms: u64,
    max_buffer_size: usize
}

impl TransactBuilder {
    #[inline]
    pub fn new_with_size(max_buffer_size: usize) -> Self {
        Self { counter: 0, data: "[\n".into(), terms: 0, total_terms: 0, max_buffer_size }
    }

    #[inline]
    pub fn next_tempid(&mut self) -> String {
        self.counter += 1;
        self.counter.to_string()
    }

    #[inline]
    pub fn add_ref_to_tmpid(&mut self, tmpid: &str, attr: &Keyword, ref_tmpid: &str) {
        write!(self.data, " [:db/add {:?} {} {:?}]\n", tmpid, attr, ref_tmpid).unwrap();
        self.terms += 1;
        self.total_terms += 1;
    }

    #[inline]
    pub fn add_inst(&mut self, tmpid: &str, attr: &Keyword, micros: i64) {
        write!(self.data, " [:db/add {:?} {} #instmicros {}]\n", tmpid, attr, micros).unwrap();
        self.terms += 1;
        self.total_terms += 1;
    }

    #[inline]
    pub fn add_str(&mut self, tmpid: &str, attr: &Keyword, val: &str) {
        // {:?} escapes some chars EDN can't parse (e.g. \'...)
        let s = val.replace("\\", "\\\\").replace("\"", "\\\"");
        write!(self.data, " [:db/add {:?} {} \"{}\"]\n", tmpid, attr, s).unwrap();
        self.terms += 1;
        self.total_terms += 1;
    }

    #[inline]
    pub fn add_long(&mut self, tmpid: &str, attr: &Keyword, val: i64) {
        write!(self.data, " [:db/add {:?} {} {}]\n", tmpid, attr, val).unwrap();
        self.terms += 1;
        self.total_terms += 1;
    }

    #[inline]
    pub fn finish(&mut self) -> &str {
        self.data.push(']');
        &self.data
    }

    #[inline]
    pub fn reset(&mut self) {
        self.terms = 0;
        self.data.clear();
        self.data.push_str("[\n")
    }

    #[inline]
    pub fn should_finish(&self) -> bool {
        self.data.len() >= self.max_buffer_size
    }

    #[inline]
    pub fn maybe_transact(&mut self, store: &mut Store) -> MentatResult<Option<mentat::TxReport>> {
        if self.should_finish() {
            Ok(self.transact(store)?)
        } else {
            Ok(None)
        }
    }

    #[inline]
    pub fn transact(&mut self, store: &mut Store) -> MentatResult<Option<mentat::TxReport>> {
        if self.terms != 0 {
            debug!("\nTransacting {} terms (total = {})", self.terms, self.total_terms);
            let res = store.transact(self.finish());
            if res.is_err() { error!("Error transacting:\n{}", self.data); }
            let report = res?;
            self.reset();
            Ok(Some(report))
        } else {
            Ok(None)
        }
    }
}

lazy_static! {

    static ref ORIGIN_PREFIX: Keyword = kw!(:origin/prefix);
    static ref ORIGIN_HOST: Keyword = kw!(:origin/host);
    static ref ORIGIN_PLACES_ID: Keyword = kw!(:origin/places_id);

    static ref PAGE_URL: Keyword = kw!(:page/url);
    static ref PAGE_ORIGIN: Keyword = kw!(:page/origin);

    static ref PAGE_TITLE: Keyword = kw!(:page/title);
    // static ref PAGE_FAVICON_URL: Keyword = kw!(:page/favicon_url);
    static ref PAGE_DESCRIPTION: Keyword = kw!(:page/description);
    static ref PAGE_PREVIEW_IMAGE_URL: Keyword = kw!(:page/preview_image_url);

    static ref VISIT_CONTEXT: Keyword = kw!(:visit/context);
    static ref VISIT_PAGE: Keyword = kw!(:visit/page);
    static ref VISIT_DATE: Keyword = kw!(:visit/date);

    static ref VISIT_SOURCE_VISIT: Keyword = kw!(:visit/source_visit);
    static ref VISIT_SYNC15_TYPE: Keyword = kw!(:visit/sync15_type);
    static ref SYNC15_HISTORY_GUID: Keyword = kw!(:sync15.history/guid);
    static ref SYNC15_HISTORY_PAGE: Keyword = kw!(:sync15.history/page);

    // static ref VISIT_SOURCE_REDIRECT: Keyword = kw!(:visit/source_redirect);
    // static ref VISIT_SOURCE_BOOKMARK: Keyword = kw!(:visit/source_bookmark);

    // Only used in `initial-data.edn`
    //
    // static ref DEVICE_NAME: Keyword = kw!(:device/name)
    // static ref DEVICE_TYPE: Keyword = kw!(:device/type)
    // static ref DEVICE_TYPE_DESKTOP: Keyword = kw!(:device.type/desktop)
    // static ref DEVICE_TYPE_MOBILE: Keyword = kw!(:device.type/mobile)
    // static ref CONTAINER_NAME: Keyword = kw!(:container/name)

    // static ref CONTEXT_DEVICE: Keyword = kw!(:context/device);
    // static ref CONTEXT_CONTAINER: Keyword = kw!(:context/container);
    // static ref CONTEXT_ID: Keyword = kw!(:context/id);

}

#[derive(Debug, Clone, Default)]
struct VisitInfo {
    // Everything else we fabricate (for reasons).
    date: i64,
    sync15_type: i8,
}

#[derive(Debug, Clone, Default)]
struct PlaceEntry {
    pub id: i64,
    pub url: String,
    pub description: Option<String>,
    pub preview_image_url: Option<String>,
    pub title: String,
    pub sync_guid: String,
    pub origin_id: i64,
    pub visits: Vec<VisitInfo>,
}

impl PlaceEntry {
    pub fn add(
        &self,
        builder: &mut TransactBuilder,
        store: &mut Store,
        context_ids: &[i64],
        origin_ids: &HashMap<i64, i64>
    ) -> Result<(), failure::Error> {
        let page_id = builder.next_tempid();
        builder.add_str(&page_id, &*PAGE_URL, &self.url);
        if let Some(origin_entid) = origin_ids.get(&self.origin_id) {
            builder.add_long(&page_id, &*PAGE_ORIGIN, *origin_entid);
        } else {
            warn!("Unknown entid? {}", self.origin_id);
        }

        builder.add_str(&page_id, &*PAGE_TITLE, &self.title);
        if let Some(desc) = &self.description {
            builder.add_str(&page_id, &*PAGE_DESCRIPTION, &desc);
        }
        if let Some(preview) = &self.preview_image_url {
            builder.add_str(&page_id, &*PAGE_PREVIEW_IMAGE_URL, &preview);
        }

        let sync15_history_id = builder.next_tempid();
        builder.add_str(&sync15_history_id, &*SYNC15_HISTORY_GUID, &self.sync_guid);
        builder.add_ref_to_tmpid(&sync15_history_id, &*SYNC15_HISTORY_PAGE, &page_id);

        let mut rng = thread_rng();
        for visit in &self.visits {
            let visit_id = builder.next_tempid();
            builder.add_ref_to_tmpid(&visit_id, &*VISIT_PAGE, &page_id);
            // unwrap is safe, only None for an empty slice.
            builder.add_long(&visit_id, &*VISIT_CONTEXT,  *rng.choose(context_ids).unwrap());
            builder.add_inst(&visit_id, &*VISIT_DATE, visit.date);
            builder.add_long(&visit_id, &*VISIT_SYNC15_TYPE, visit.sync15_type as i64);
            // Point the visit at itself. This doesn't really matter, but
            // pointing at another visit would require us keep a huge hashmap in
            // memory, or to keep the places id on the visit as a unique
            // identity which we use as a lookup ref, which will effect the db
            // size a lot in a way we wouldn't need to in reality.
            builder.add_ref_to_tmpid(&visit_id, &*VISIT_SOURCE_VISIT, &visit_id);
        }

        // not one tx per visit anymore (and doing per place instead) because
        // the bookkeeping/separation required is too annoying.
        builder.maybe_transact(store)?;
        Ok(())
    }

    pub fn from_row(row: &Row) -> PlaceEntry {
        PlaceEntry {
            id: row.get("place_id"),
            url: row.get("place_url"),
            origin_id: row.get("place_origin_id"),
            sync_guid: row.get("place_guid"),
            description: row.get("place_description"),
            preview_image_url: row.get("place_preview_image_url"),
            title: row.get::<_, Option<String>>("place_title").unwrap_or("".into()),
            visits: vec![VisitInfo {
                date: row.get("visit_date"),
                sync15_type: row.get("visit_type"),
            }],
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlacesToMentat {
    pub mentat_db_path: PathBuf,
    pub places_db_path: PathBuf,
    pub realistic: bool,
}

fn read_file(path: &str) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut string = String::with_capacity((file.metadata()?.len() + 1) as usize);
    file.read_to_string(&mut string)?;
    Ok(string)
}

impl PlacesToMentat {
    pub fn run(self) -> Result<(), failure::Error> {

        debug!("Copying places.sqlite to a temp file for reading");
        let temp_dir = tempfile::tempdir()?;
        let temp_places_path = temp_dir.path().join("places.sqlite");

        fs::copy(&self.places_db_path, &temp_places_path)?;
        let places = Connection::open_with_flags(&temp_places_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;

        // New versions of mentat kill open_empty, and we already know this is empty.
        let mut store = Store::open(self.mentat_db_path.to_str().unwrap())?;

        debug!("Transacting initial schema");

        let initial_schema = read_file("places-schema.edn")?;
        let initial_data = read_file("initial-data.edn")?;

        store.transact(&initial_schema)?;
        store.transact(&initial_data)?;

        let max_buffer_size = if self.realistic { 0 } else { 1024 * 1024 * 1024 * 1024 };
        let mut builder = TransactBuilder::new_with_size(max_buffer_size);

        let origin_ids = {
            let mut origins_stmt = places.prepare("SELECT id, prefix, host FROM moz_origins")?;
            let origins = origins_stmt.query_map(&[], |row| {
                (row.get::<_, i64>("id"),
                 row.get::<_, String>("prefix"),
                 row.get::<_, String>("host"))
            })?.collect::<Result<Vec<_>, _>>()?;

            println!("Adding {} origins...", origins.len());
            let temp_ids = origins.into_iter().map(|(id, prefix, host)| {
                let tmpid = builder.next_tempid();
                builder.add_str(&tmpid, &*ORIGIN_PREFIX, &host);
                builder.add_str(&tmpid, &*ORIGIN_HOST, &prefix);
                (id, tmpid)
            }).collect::<Vec<(i64, String)>>();
            if let Some(tx_report) = builder.transact(&mut store)? {
                let mut table: HashMap<i64, i64> = HashMap::with_capacity(temp_ids.len());
                for (origin_id, tmpid) in temp_ids {
                    let entid = tx_report.tempids.get(&tmpid).unwrap();
                    table.insert(origin_id, *entid);
                }
                table
            } else {
                HashMap::default()
            }
        };

        let context_ids = store.q_once("[:find [?e ...] :where [?e :context/device _]]", None)?
            .results
            .into_coll()?
            .into_iter()
            .map(|binding| binding.into_entid().unwrap())
            .collect::<Vec<_>>();

        let (place_count, visit_count) = {
            let mut stmt = places.prepare("SELECT count(*) FROM moz_places").unwrap();
            let mut rows = stmt.query(&[]).unwrap();
            let ps: i64 = rows.next().unwrap()?.get(0);

            let mut stmt = places.prepare("SELECT count(*) FROM moz_historyvisits").unwrap();
            let mut rows = stmt.query(&[]).unwrap();
            let vs: i64 = rows.next().unwrap()?.get(0);
            (ps, vs)
        };

        println!("Querying {} places ({} visits)", place_count, visit_count);
        { // Scope borrow of stmt
            let mut stmt = places.prepare("
                SELECT
                    p.id                as place_id,
                    p.url               as place_url,
                    p.description       as place_description,
                    p.preview_image_url as place_preview_image_url,
                    p.title             as place_title,
                    p.origin_id         as place_origin_id,
                    p.guid              as place_guid,
                    v.visit_date        as visit_date,
                    v.visit_type        as visit_type
                FROM moz_places p
                JOIN moz_historyvisits v
                    ON p.id = v.place_id
                ORDER BY p.id
            ")?;

            let mut current_place = PlaceEntry { id: -1, .. PlaceEntry::default() };

            let mut so_far = 0;
            let mut rows = stmt.query(&[])?;

            while let Some(row_or_error) = rows.next() {
                let row = row_or_error?;
                let id: i64 = row.get("place_id");
                if current_place.id == id {
                    current_place.visits.push(VisitInfo {
                        date: row.get("visit_date"),
                        sync15_type: row.get("visit_type"),
                    });
                    continue;
                }

                if current_place.id >= 0 {
                    current_place.add(&mut builder, &mut store, &context_ids, &origin_ids)?;
                    print!("\rProcessing {} / {} places (approx.)", so_far, place_count);
                    io::stdout().flush()?;
                    so_far += 1;
                }
                current_place = PlaceEntry::from_row(&row);
            }

            if current_place.id >= 0 {
                current_place.add(&mut builder, &mut store, &context_ids, &origin_ids)?;
                println!("\rProcessing {} / {} places (approx.)", so_far + 1, place_count);
            }
            builder.transact(&mut store)?;

            println!("Vacuuming mentat DB");

            let mentat_sqlite_conn = store.dismantle().0;
            mentat_sqlite_conn.execute("VACUUM", &[])?;
            println!("Done!");
        }
        drop(places);
        let mentat_size = File::open(&self.mentat_db_path)?.metadata()?.len();
        let places_size = File::open(&self.places_db_path)?.metadata()?.len();
        println!("Initial places size: {}", humanize_size(places_size));
        println!("Final mentat size: {}", humanize_size(mentat_size));

        Ok(())
    }

}

