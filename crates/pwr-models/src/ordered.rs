//! Ordering the Hub's catalogue by parameters, which the Hub cannot sort by.
//!
//! Sorting the pages already fetched ordered only those: "smallest first"
//! showed the smallest of the most downloaded twenty, not the smallest
//! model there is. The Hub can, however, *filter* by a parameter range. So
//! the catalogue is walked in consecutive ranges -- up from the smallest, or
//! down from the largest -- and a range is taken only once the Hub has
//! listed all of it: too many for one listing, and it is halved until it
//! fits. Every model in a complete range is known, so sorting it is exact,
//! and ranges follow one another, so the order holds across the whole
//! catalogue and every page.
//!
//! Listings are cheap (no file tree, no config); only the models a page
//! shows are enriched. A model the Hub lists no parameter count for cannot
//! be placed, and the Hub leaves it out of any range: these orders do not
//! show it.

use crate::CatalogOrder;
use crate::catalog::HubModel;
use crate::hub::HubError;
use serde::{Deserialize, Serialize};

/// Repositories one listing asks for.
pub const LISTING_LIMIT: usize = 50;
/// Past this, a range is not halved again: more than a listing's worth of
/// models share it (the quantizations of one model can), and it is read
/// page by page instead.
const NARROWEST: u64 = 1_000_000;
/// The largest parameter count a walk down starts from when nothing
/// narrower bounds it.
const LARGEST: u64 = 4_000_000_000_000;
/// Listings one page may spend, so a sparse catalogue answers in bounded
/// time; the walk carries on from where it stopped on the next page.
const LISTINGS_PER_PAGE: usize = 24;
/// Listings one range read page by page may take.
const PAGES_PER_RANGE: usize = 20;
/// A walk's first range: this many parameters, then doubled while ranges
/// come back sparse.
const FIRST_WIDTH: u64 = 1_000_000_000;

/// The Hub's listing of the search in hand, within a parameter range.
pub trait Listing {
    #[allow(async_fn_in_trait)]
    async fn list(
        &self,
        min: u64,
        max: u64,
        cursor: Option<&str>,
    ) -> Result<(Vec<HubModel>, Option<String>), HubError>;
}

/// Where a walk stands, carried in the page cursor. Stateless on the core's
/// side: a restarted core carries on from the cursor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Walk {
    order: CatalogOrder,
    /// The next unread bound: the lowest count not yet listed going up, the
    /// highest going down.
    edge: u64,
    /// The far end of the walk, inclusive.
    end: u64,
    width: u64,
    /// Listed, from complete ranges, in order, not yet shown.
    listed: Vec<HubModel>,
    done: bool,
}

impl Walk {
    /// A walk in `order` within the filters' bounds, and below `limit` --
    /// the most parameters a model that could fit this machine may have.
    pub fn start(
        order: CatalogOrder,
        limit: Option<u64>,
        (min, max): (Option<u64>, Option<u64>),
    ) -> Self {
        let low = min.unwrap_or(0);
        let high = [max, limit].into_iter().flatten().min().unwrap_or(LARGEST);
        let (edge, end) = match order {
            CatalogOrder::SmallestFirst => (low, high),
            CatalogOrder::LargestFirst => (high, low),
        };
        Walk {
            order,
            edge,
            end,
            width: FIRST_WIDTH,
            listed: Vec::new(),
            done: low > high,
        }
    }

    /// Whether every model in its bounds has been handed out.
    pub fn finished(&self) -> bool {
        self.done && self.listed.is_empty()
    }

    /// The next models in order, up to `wanted`: fewer only when the walk
    /// ends, or when a page's listings ran out and it goes on next time.
    pub async fn next(
        &mut self,
        listing: &impl Listing,
        wanted: usize,
    ) -> Result<Vec<HubModel>, HubError> {
        fill(self, listing, wanted).await?;
        Ok(self.listed.drain(..self.listed.len().min(wanted)).collect())
    }
}

async fn fill(walk: &mut Walk, listing: &impl Listing, wanted: usize) -> Result<(), HubError> {
    let mut spent = 0;
    while walk.listed.len() < wanted && !walk.done && spent < LISTINGS_PER_PAGE {
        let up = walk.order == CatalogOrder::SmallestFirst;
        let (min, max) = if up {
            (
                walk.edge,
                walk.edge.saturating_add(walk.width - 1).min(walk.end),
            )
        } else {
            (
                walk.edge.saturating_sub(walk.width - 1).max(walk.end),
                walk.edge,
            )
        };
        let (mut models, more) = listing.list(min, max, None).await?;
        spent += 1;
        if let Some(mut cursor) = more {
            if max - min + 1 > NARROWEST {
                // Too many to know them all from one listing: halve the range.
                walk.width = (max - min + 1).div_ceil(2);
                continue;
            }
            // More models share this narrow range than one listing holds:
            // read it to its end.
            for _ in 0..PAGES_PER_RANGE {
                let (next, after) = listing.list(min, max, Some(&cursor)).await?;
                spent += 1;
                models.extend(next);
                match after {
                    Some(after) => cursor = after,
                    None => break,
                }
            }
        }
        models.sort_by(|a, b| {
            let (x, y) = (count(a), count(b));
            let by_size = if up { x.cmp(&y) } else { y.cmp(&x) };
            by_size.then(b.downloads.cmp(&a.downloads))
        });
        let before = walk.listed.len();
        for model in models {
            if !walk
                .listed
                .iter()
                .any(|seen| seen.repository == model.repository)
            {
                walk.listed.push(model);
            }
        }
        // A sparse range: the next one is wider.
        if walk.listed.len() - before < LISTING_LIMIT / 4 {
            walk.width = walk.width.saturating_mul(2);
        }
        if up {
            if max >= walk.end {
                walk.done = true;
            } else {
                walk.edge = max + 1;
            }
        } else if min <= walk.end {
            walk.done = true;
        } else {
            walk.edge = min - 1;
        }
    }
    Ok(())
}

fn count(model: &HubModel) -> u64 {
    model
        .gguf_parameters
        .or(model.safetensors_parameters)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    /// A Hub of `models` (name, parameters, downloads) that lists a range in
    /// pages of `LISTING_LIMIT`, most downloaded first, as the Hub does.
    struct Hub {
        models: Vec<HubModel>,
        listings: Cell<usize>,
    }

    impl Hub {
        fn new(models: &[(&str, u64, u64)]) -> Self {
            Hub {
                models: models
                    .iter()
                    .map(|(name, parameters, downloads)| HubModel {
                        repository: format!("org/{name}"),
                        safetensors_parameters: Some(*parameters),
                        downloads: Some(*downloads),
                        ..Default::default()
                    })
                    .collect(),
                listings: Cell::new(0),
            }
        }
    }

    impl Listing for Hub {
        async fn list(
            &self,
            min: u64,
            max: u64,
            cursor: Option<&str>,
        ) -> Result<(Vec<HubModel>, Option<String>), HubError> {
            self.listings.set(self.listings.get() + 1);
            let mut inside: Vec<HubModel> = self
                .models
                .iter()
                .filter(|model| (min..=max).contains(&count(model)))
                .cloned()
                .collect();
            inside.sort_by_key(|model| std::cmp::Reverse(model.downloads));
            let from: usize = cursor.map_or(0, |cursor| cursor.parse().unwrap());
            let to = (from + LISTING_LIMIT).min(inside.len());
            let next = (to < inside.len()).then(|| to.to_string());
            Ok((inside[from..to].to_vec(), next))
        }
    }

    fn names(models: &[HubModel]) -> Vec<String> {
        models
            .iter()
            .map(|model| model.repository.trim_start_matches("org/").to_owned())
            .collect()
    }

    async fn walk(hub: &Hub, order: CatalogOrder, limit: Option<u64>) -> Vec<String> {
        let mut walk = Walk::start(order, limit, (None, None));
        let mut all = Vec::new();
        while !walk.finished() {
            // Carried between pages as the cursor carries it.
            let text = serde_json::to_string(&walk).unwrap();
            walk = serde_json::from_str(&text).unwrap();
            all.extend(names(&walk.next(hub, 20).await.unwrap()));
        }
        all
    }

    const B: u64 = 1_000_000_000;

    #[tokio::test]
    async fn the_smallest_come_first_across_the_whole_catalogue() {
        // The most downloaded are large: sorting the first page found an 8B
        // as "the smallest" while a 0.5B sat further down the Hub's order.
        let mut models = vec![
            ("big", 70 * B, 9_000),
            ("qwen8", 8 * B, 8_000),
            ("tiny", B / 2, 1),
        ];
        let filler: Vec<(String, u64, u64)> = (0..120)
            .map(|n| (format!("m{n}"), 3 * B + n * 10_000_000, 100 + n))
            .collect();
        models.extend(filler.iter().map(|(name, p, d)| (name.as_str(), *p, *d)));
        let hub = Hub::new(&models);
        let order = walk(&hub, CatalogOrder::SmallestFirst, None).await;
        assert_eq!(order.len(), 123, "every model, once");
        assert_eq!(order[0], "tiny");
        assert_eq!(order.last().unwrap(), "big");
        let counts: Vec<u64> = order
            .iter()
            .map(|name| {
                count(
                    hub.models
                        .iter()
                        .find(|m| m.repository == format!("org/{name}"))
                        .unwrap(),
                )
            })
            .collect();
        assert!(
            counts.windows(2).all(|pair| pair[0] <= pair[1]),
            "{counts:?}"
        );
    }

    #[tokio::test]
    async fn the_largest_first_start_from_what_could_fit() {
        let hub = Hub::new(&[
            ("huge", 480 * B, 50),
            ("qwen32", 32 * B, 10),
            ("qwen8", 8 * B, 99),
        ]);
        assert_eq!(
            walk(&hub, CatalogOrder::LargestFirst, None).await,
            ["huge", "qwen32", "qwen8"]
        );
        // A 64 GB machine: no 480B model can load, so none is walked through.
        assert_eq!(
            walk(&hub, CatalogOrder::LargestFirst, Some(340 * B)).await,
            ["qwen32", "qwen8"]
        );
    }

    #[tokio::test]
    async fn many_models_of_one_size_are_read_to_the_end() {
        // Quantizations of one model share its parameter count: more than a
        // listing holds, in a range that cannot be halved further.
        let same: Vec<(String, u64, u64)> = (0..130).map(|n| (format!("q{n}"), 8 * B, n)).collect();
        let mut models: Vec<(&str, u64, u64)> =
            same.iter().map(|(n, p, d)| (n.as_str(), *p, *d)).collect();
        models.push(("small", 4 * B, 1));
        let hub = Hub::new(&models);
        let order = walk(&hub, CatalogOrder::SmallestFirst, None).await;
        assert_eq!(order.len(), 131);
        assert_eq!(order[0], "small");
    }

    #[test]
    fn the_interface_names_the_orders_the_core_reads() {
        for (name, order) in [
            ("smallestFirst", CatalogOrder::SmallestFirst),
            ("largestFirst", CatalogOrder::LargestFirst),
        ] {
            let filters: crate::Filters =
                serde_json::from_value(serde_json::json!({ "order": name })).unwrap();
            assert_eq!(filters.order, Some(order));
        }
        assert!(
            serde_json::from_value::<crate::Filters>(serde_json::json!({ "order": "name" }))
                .is_err()
        );
    }
}
