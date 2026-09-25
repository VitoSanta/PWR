//! Filling a page of the catalogue with models that pass its filters.
//!
//! A page used to be twenty listings, each enriched (a file tree and a
//! config, two requests), then filtered: whatever was left was the page.
//! With "fits this machine" that was often nothing -- largest first listed
//! twenty models too large for any Mac and showed "No matches on this page".
//!
//! Now a page is filled: listings are taken from the source -- the Hub's own
//! order, or a walk by size ([`crate::ordered`]) -- in order; a listing that
//! cannot pass the filters, as far as the listing alone shows, is dropped
//! before anything is spent on it ([`could_pass`]); the rest are enriched a
//! few at a time and filtered exactly, until the page is full or its budget
//! of requests is spent. Listings taken but not yet shown ride in the cursor
//! to the next page, so nothing is skipped and the order holds.

use crate::catalog::{Format, HubModel};
use crate::hub::{HubError, HubErrorKind};
use crate::ordered::{self, Listing};
use crate::{CatalogOrder, Filters, SEARCH_LIMIT};
use serde::{Deserialize, Serialize};

/// Enrichments one page may spend: each is two requests, and the Hub limits
/// how many an address makes (measured on the Mac: seven searches in a row
/// were rate-limited at sixty a page).
const ENRICHED_PER_PAGE: usize = 40;
/// Listings (of the Hub's order, or walk batches) one page may read.
const READS_PER_PAGE: usize = 24;
/// A page that has something stops reading after this long; one that has
/// nothing yet goes on to [`LONGEST`]. A walk down past models too large
/// for this Mac read for four batches and returned an empty page (measured
/// on the Mac): a page that says nothing was found while more exists is the
/// wrong answer, so the budget is time, not a count of batches.
const ENOUGH: std::time::Duration = std::time::Duration::from_secs(6);
const LONGEST: std::time::Duration = std::time::Duration::from_secs(20);
/// Bits per weight below which no model is stored: the floor a listing's
/// size is estimated at when its name does not say its quantization.
const FLOOR_BITS: f64 = 1.5;

/// The catalogue a page is filled from.
pub trait Catalogue: Listing {
    /// What a page shows.
    type Entry;
    /// The search in the Hub's own order, a listing at a time.
    #[allow(async_fn_in_trait)]
    async fn page(&self, cursor: Option<&str>)
    -> Result<(Vec<HubModel>, Option<String>), HubError>;
    /// Listings enriched and filtered exactly, in the order given.
    #[allow(async_fn_in_trait)]
    async fn enrich(&self, models: Vec<HubModel>) -> Result<Vec<Self::Entry>, HubError>;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
enum Source {
    Hub { cursor: Option<String>, done: bool },
    Walk(ordered::Walk),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct State {
    source: Source,
    /// Listed, kept by `could_pass`, not yet enriched: shown next.
    pending: Vec<HubModel>,
}

const PREFIX: &str = "pwr-page:";

/// One page: the entries to show and the cursor for the next, if any.
/// `keep` is [`could_pass`] with the search's filters and host bound in.
pub async fn fill<C: Catalogue>(
    catalogue: &C,
    order: Option<CatalogOrder>,
    walk_limit: Option<u64>,
    bounds: (Option<u64>, Option<u64>),
    cursor: Option<&str>,
    keep: impl Fn(&HubModel) -> bool,
) -> Result<(Vec<C::Entry>, Option<String>), HubError> {
    let mut state = match cursor {
        Some(cursor) => match cursor.strip_prefix(PREFIX) {
            Some(json) => serde_json::from_str(json).map_err(|_| {
                HubError::new(
                    HubErrorKind::Invalid,
                    "This list could not be continued; search again.",
                )
            })?,
            // A cursor from before pages were filled: the Hub's own.
            None => State {
                source: Source::Hub {
                    cursor: Some(cursor.to_owned()),
                    done: false,
                },
                pending: Vec::new(),
            },
        },
        None => State {
            source: match order {
                Some(order) => Source::Walk(ordered::Walk::start(order, walk_limit, bounds)),
                None => Source::Hub {
                    cursor: None,
                    done: false,
                },
            },
            pending: Vec::new(),
        },
    };
    let mut shown = Vec::new();
    let mut enriched = 0;
    let mut reads = 0;
    let started = std::time::Instant::now();
    let out_of_time = |shown: usize| {
        let spent = started.elapsed();
        spent >= LONGEST || (shown > 0 && spent >= ENOUGH)
    };
    while shown.len() < SEARCH_LIMIT && enriched < ENRICHED_PER_PAGE {
        if state.pending.is_empty() {
            let (listed, exhausted) = match &mut state.source {
                Source::Hub { done: true, .. } => (Vec::new(), true),
                Source::Hub { cursor, done } => {
                    if reads >= READS_PER_PAGE || out_of_time(shown.len()) {
                        break;
                    }
                    reads += 1;
                    let (models, next) = match catalogue.page(cursor.as_deref()).await {
                        Ok(read) => read,
                        Err(error) if shown.is_empty() => return Err(error),
                        // What the page has is shown, and the next goes on
                        // from here.
                        Err(_) => break,
                    };
                    *done = next.is_none();
                    *cursor = next;
                    (models, false)
                }
                Source::Walk(walk) => {
                    if walk.finished() {
                        (Vec::new(), true)
                    } else {
                        if reads >= READS_PER_PAGE || out_of_time(shown.len()) {
                            break;
                        }
                        reads += 1;
                        match walk.next(catalogue, ordered::LISTING_LIMIT).await {
                            Ok(listed) => (listed, false),
                            Err(error) if shown.is_empty() => return Err(error),
                            Err(_) => break,
                        }
                    }
                }
            };
            if exhausted {
                break;
            }
            state
                .pending
                .extend(listed.into_iter().filter(|model| keep(model)));
            continue;
        }
        // Exactly what fills the page if all pass: any that do not are
        // replaced by the next ones, and none is enriched and then dropped.
        let wanted = (SEARCH_LIMIT - shown.len())
            .min(ENRICHED_PER_PAGE - enriched)
            .min(state.pending.len());
        let chunk: Vec<HubModel> = state.pending.drain(..wanted).collect();
        enriched += chunk.len();
        match catalogue.enrich(chunk.clone()).await {
            Ok(entries) => shown.extend(entries),
            Err(error) => {
                // Rate-limited: these go back first in line, shown by the
                // next page -- or this one is the error, when it has nothing.
                state.pending.splice(0..0, chunk);
                if shown.is_empty() {
                    return Err(error);
                }
                break;
            }
        }
    }
    let more = !state.pending.is_empty()
        || match &state.source {
            Source::Hub { done, .. } => !done,
            Source::Walk(walk) => !walk.finished(),
        };
    let next = more.then(|| {
        format!(
            "{PREFIX}{}",
            serde_json::to_string(&state).unwrap_or_default()
        )
    });
    Ok((shown, next))
}

/// Whether a listing could pass the filters, from what the listing shows:
/// its parameter count and the quantization its name states. Only what
/// certainly cannot is dropped -- a model whose name states no quantization
/// is sized at the lowest precision weights are stored at -- and an
/// installed model is always kept, as the exact filter keeps it.
pub fn could_pass(
    model: &HubModel,
    format: Format,
    filters: &Filters,
    budget: Option<u64>,
    installed: &[String],
) -> bool {
    if installed
        .iter()
        .any(|reference| reference == &model.repository)
    {
        return true;
    }
    // A quantization asked for, and a different one in the name.
    if let (Some(wanted), Some(stated)) = (
        filters.quantization.as_deref().and_then(asked_bits),
        stated_bits(&model.repository),
    ) && format == Format::Mlx
        && wanted != stated
    {
        return false;
    }
    let Some(parameters) = model.listed_parameters() else {
        return true;
    };
    // GGUF repositories hold several quantizations: the lightest decides.
    let bits = match format {
        Format::Mlx => stated_bits(&model.repository).unwrap_or(FLOOR_BITS),
        Format::Gguf => FLOOR_BITS,
    };
    let least = (parameters as f64 * bits / 8.0) as u64;
    let fits = !filters.compatible_only
        || budget.is_none_or(|budget| least + crate::fit::RUNTIME_OVERHEAD_BYTES <= budget);
    let small_enough = filters.max_bytes.is_none_or(|max| least <= max);
    fits && small_enough
}

/// The bits a quantization filter asks for -- "4-bit", "4bit", "q4",
/// "mxfp4", "bf16" -- or `None` when it names no width.
fn asked_bits(filter: &str) -> Option<f64> {
    let filter = filter.trim().to_ascii_lowercase();
    if filter.contains("16") && filter.contains('f') {
        return Some(16.0);
    }
    if filter.contains("fp4") {
        return Some(4.0);
    }
    let digits: String = filter
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(char::is_ascii_digit)
        .collect();
    let bits: f64 = digits.parse().ok()?;
    (filter.contains("bit") || filter.starts_with('q')).then_some(bits)
}

/// The bits per weight a repository's name states: `-4bit`, `-8bit`,
/// `bf16`, `mxfp4`... `None` when it states none.
pub fn stated_bits(repository: &str) -> Option<f64> {
    let name = repository.rsplit('/').next()?.to_ascii_lowercase();
    let tokens: Vec<&str> = name.split(['-', '_', '.']).collect();
    let has = |token: &str| tokens.contains(&token);
    for (bits, marks) in [
        (16.0, &["bf16", "fp16", "f16", "16bit"][..]),
        (8.0, &["8bit", "fp8", "int8", "q8"][..]),
        (6.0, &["6bit", "q6"][..]),
        (5.0, &["5bit", "q5"][..]),
        (
            4.0,
            &["4bit", "int4", "q4", "mxfp4", "nvfp4", "dwq", "awq", "gptq"][..],
        ),
        (3.0, &["3bit", "q3"][..]),
        (2.0, &["2bit", "q2"][..]),
    ] {
        if marks.iter().any(|mark| has(mark)) {
            return Some(bits);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    const B: u64 = 1_000_000_000;
    const GIB: u64 = 1024 * 1024 * 1024;

    /// A Hub whose models are listed most downloaded first, twenty a page;
    /// enriching one records it, and the exact filter keeps those whose name
    /// is in `fits`.
    struct Fake {
        models: Vec<HubModel>,
        fits: Vec<&'static str>,
        enriched: RefCell<Vec<String>>,
        /// Enrichments answered before the Hub starts rate-limiting.
        limit_after: std::cell::Cell<Option<usize>>,
    }

    impl Fake {
        fn new(models: Vec<HubModel>, fits: Vec<&'static str>) -> Self {
            Fake {
                models,
                fits,
                enriched: RefCell::new(Vec::new()),
                limit_after: std::cell::Cell::new(None),
            }
        }
    }

    fn model(name: &str, parameters: u64, downloads: u64) -> HubModel {
        HubModel {
            repository: format!("org/{name}"),
            safetensors_parameters: Some(parameters),
            downloads: Some(downloads),
            ..Default::default()
        }
    }

    impl Listing for Fake {
        async fn list(
            &self,
            min: u64,
            max: u64,
            cursor: Option<&str>,
        ) -> Result<(Vec<HubModel>, Option<String>), HubError> {
            let inside: Vec<HubModel> = self
                .models
                .iter()
                .filter(|m| (min..=max).contains(&m.safetensors_parameters.unwrap()))
                .cloned()
                .collect();
            Ok(slice(inside, cursor, ordered::LISTING_LIMIT))
        }
    }

    impl Catalogue for Fake {
        type Entry = String;
        async fn page(
            &self,
            cursor: Option<&str>,
        ) -> Result<(Vec<HubModel>, Option<String>), HubError> {
            Ok(slice(self.models.clone(), cursor, SEARCH_LIMIT))
        }
        async fn enrich(&self, models: Vec<HubModel>) -> Result<Vec<String>, HubError> {
            if let Some(left) = self.limit_after.get() {
                if left == 0 {
                    return Err(HubError::new(HubErrorKind::RateLimited, "rate-limited"));
                }
                self.limit_after.set(Some(left - 1));
            }
            let names: Vec<String> = models
                .iter()
                .map(|m| m.repository.trim_start_matches("org/").to_owned())
                .collect();
            self.enriched.borrow_mut().extend(names.iter().cloned());
            Ok(names
                .into_iter()
                .filter(|name| self.fits.contains(&name.as_str()))
                .collect())
        }
    }

    fn slice(
        mut models: Vec<HubModel>,
        cursor: Option<&str>,
        size: usize,
    ) -> (Vec<HubModel>, Option<String>) {
        models.sort_by_key(|m| std::cmp::Reverse(m.downloads));
        let from: usize = cursor.map_or(0, |c| c.parse().unwrap());
        let to = (from + size).min(models.len());
        (
            models[from..to].to_vec(),
            (to < models.len()).then(|| to.to_string()),
        )
    }

    /// 64 GB of unified memory: 48 GiB for a model.
    const BUDGET: u64 = 48 * GIB;

    fn keep(filters: &Filters) -> impl Fn(&HubModel) -> bool + '_ {
        |m| could_pass(m, Format::Mlx, filters, Some(BUDGET), &[])
    }

    #[tokio::test]
    async fn largest_first_on_this_mac_starts_at_what_fits_and_fills_the_page() {
        // The screenshot: the largest models first, none of which a 64 GB
        // Mac can load, and a page with nothing on it.
        let mut models = vec![
            model("Qwen3-235B-A22B-4bit", 235 * B, 900),
            model("gpt-oss-120b-MLX-8bit", 120 * B, 800),
            model("Llama-70B-4bit", 70 * B, 10),
        ];
        let mut fits = vec!["Llama-70B-4bit"];
        let names: Vec<String> = (0..30).map(|n| format!("m{n}-32B-4bit")).collect();
        for (n, name) in names.iter().enumerate() {
            models.push(model(name, 32 * B - n as u64, 5));
        }
        fits.extend(
            names
                .iter()
                .map(|name| &*Box::leak(name.clone().into_boxed_str())),
        );
        let fake = Fake::new(models, fits);
        let filters = Filters {
            compatible_only: true,
            ..Default::default()
        };
        let (page, next) = fill(
            &fake,
            Some(CatalogOrder::LargestFirst),
            None,
            (None, None),
            None,
            keep(&filters),
        )
        .await
        .unwrap();
        assert_eq!(page.len(), SEARCH_LIMIT, "a full page");
        assert_eq!(page[0], "Llama-70B-4bit", "the largest that can load here");
        assert!(next.is_some());
        // Nothing was spent on models that could never load.
        let enriched = fake.enriched.borrow();
        assert!(
            !enriched
                .iter()
                .any(|name| name.contains("235B") || name.contains("120b"))
        );
    }

    #[tokio::test]
    async fn the_hubs_order_is_read_on_until_the_page_is_full() {
        // The most downloaded are mostly models this Mac cannot load: one
        // page of the Hub's order used to leave two cards.
        let mut models = Vec::new();
        let mut fits = Vec::new();
        for n in 0..60u64 {
            let big = n % 3 != 0;
            let name: &'static str = Box::leak(
                format!("m{n}-{}", if big { "480B-4bit" } else { "8B-4bit" }).into_boxed_str(),
            );
            models.push(model(name, if big { 480 * B } else { 8 * B }, 1000 - n));
            if !big {
                fits.push(name);
            }
        }
        let fake = Fake::new(models, fits);
        let filters = Filters {
            compatible_only: true,
            ..Default::default()
        };
        let (page, next) = fill(&fake, None, None, (None, None), None, keep(&filters))
            .await
            .unwrap();
        assert_eq!(
            page.len(),
            20,
            "every fitting model, from three of the Hub's pages"
        );
        assert!(page.iter().all(|name| name.contains("8B")));
        // Still in the Hub's order.
        let order: Vec<u64> = page
            .iter()
            .map(|name| name[1..name.find('-').unwrap()].parse().unwrap())
            .collect();
        assert!(order.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(next.is_none(), "the Hub had no more");
    }

    #[tokio::test]
    async fn a_page_is_not_left_empty_while_more_could_fit() {
        // Three hundred of the Hub's most downloaded cannot load here: the
        // page reads on until it finds the ones that can.
        let mut models: Vec<HubModel> = (0..300u64)
            .map(|n| model(&format!("big{n}-480B-4bit"), 480 * B, 10_000 - n))
            .collect();
        models.push(model("small-8B-4bit", 8 * B, 1));
        let fake = Fake::new(models, vec!["small-8B-4bit"]);
        let filters = Filters {
            compatible_only: true,
            ..Default::default()
        };
        let (page, _) = fill(&fake, None, None, (None, None), None, keep(&filters))
            .await
            .unwrap();
        assert_eq!(page, ["small-8B-4bit"]);
        assert!(
            fake.enriched
                .borrow()
                .iter()
                .all(|name| !name.contains("480B"))
        );
    }

    #[tokio::test]
    async fn what_a_page_did_not_show_comes_first_on_the_next() {
        let models: Vec<HubModel> = (0..45u64)
            .map(|n| model(&format!("m{n}"), 8 * B, 100 - n))
            .collect();
        let fits: Vec<&'static str> = (0..45)
            .map(|n| &*Box::leak(format!("m{n}").into_boxed_str()))
            .collect();
        let fake = Fake::new(models, fits);
        let filters = Filters::default();
        let mut all = Vec::new();
        let mut cursor = None;
        loop {
            let (page, next) = fill(
                &fake,
                None,
                None,
                (None, None),
                cursor.as_deref(),
                keep(&filters),
            )
            .await
            .unwrap();
            all.extend(page);
            match next {
                Some(next) => cursor = Some(next),
                None => break,
            }
        }
        let expected: Vec<String> = (0..45).map(|n| format!("m{n}")).collect();
        assert_eq!(all, expected, "every model once, in the Hub's order");
    }

    #[tokio::test]
    async fn a_rate_limit_shows_what_the_page_has_and_the_next_goes_on() {
        let models: Vec<HubModel> = (0..30u64)
            .map(|n| model(&format!("m{n}"), 8 * B, 100 - n))
            .collect();
        let fits: Vec<&'static str> = (0..30)
            .map(|n| &*Box::leak(format!("m{n}").into_boxed_str()))
            .collect();
        let fake = Fake::new(models, fits);
        let filters = Filters::default();
        // The first enrichment is answered; the second is rate-limited.
        fake.limit_after.set(Some(1));
        let (first, next) = fill(&fake, None, None, (None, None), None, keep(&filters))
            .await
            .unwrap();
        assert_eq!(first.len(), 20);
        // Once the Hub answers again, nothing was skipped.
        fake.limit_after.set(None);
        let (second, _) = fill(
            &fake,
            None,
            None,
            (None, None),
            next.as_deref(),
            keep(&filters),
        )
        .await
        .unwrap();
        assert_eq!(second[0], "m20");
        assert_eq!(second.len(), 10);
        // A page that has nothing yet is the error, said as it is.
        fake.limit_after.set(Some(0));
        let refused = fill(&fake, None, None, (None, None), None, keep(&filters)).await;
        assert_eq!(refused.unwrap_err().kind, HubErrorKind::RateLimited);
    }

    #[test]
    fn a_quantization_asked_for_drops_the_names_that_state_another() {
        let filters = Filters {
            quantization: Some("4-bit".into()),
            ..Default::default()
        };
        let pass =
            |name: &str| could_pass(&model(name, 8 * B, 0), Format::Mlx, &filters, None, &[]);
        assert!(pass("Qwen3-8B-4bit"));
        assert!(pass("gpt-oss-20b-mxfp4"));
        assert!(!pass("Qwen3-8B-8bit"));
        assert!(!pass("Jaja-small-2bit-mlx"));
        // A name that states nothing is kept: its config decides.
        assert!(pass("Qwen3-8B-MLX"));
        assert_eq!(asked_bits("Q4_K"), Some(4.0));
        assert_eq!(asked_bits("bf16"), Some(16.0));
        assert_eq!(asked_bits("DWQ"), None);
    }

    #[test]
    fn only_what_certainly_cannot_fit_is_dropped() {
        let filters = Filters {
            compatible_only: true,
            ..Default::default()
        };
        let pass = |name: &str, parameters| {
            could_pass(
                &model(name, parameters, 0),
                Format::Mlx,
                &filters,
                Some(BUDGET),
                &[],
            )
        };
        assert!(!pass("Qwen3-235B-A22B-4bit", 235 * B));
        assert!(pass("Llama-3.3-70B-Instruct-4bit", 70 * B));
        assert!(!pass("Llama-3.3-70B-Instruct-bf16", 70 * B));
        // No quantization in the name: sized at the floor, so kept.
        assert!(pass("some-120B-model", 120 * B));
        assert!(!pass("some-480B-model", 480 * B));
        // Installed is always kept.
        assert!(could_pass(
            &model("Qwen3-235B-A22B-4bit", 235 * B, 0),
            Format::Mlx,
            &filters,
            Some(BUDGET),
            &["org/Qwen3-235B-A22B-4bit".to_owned()]
        ));
        assert_eq!(stated_bits("mlx-community/Qwen3-8B-4bit-DWQ"), Some(4.0));
        assert_eq!(
            stated_bits("lmstudio-community/gpt-oss-20b-MLX-8bit"),
            Some(8.0)
        );
        assert_eq!(stated_bits("org/Model-bf16"), Some(16.0));
        assert_eq!(stated_bits("org/Qwen3-4B"), None);
    }
}
