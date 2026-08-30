//! Density-driven ad/nature interleaving shared by the feed and search pages.
//!
//! Both surfaces previously hard-coded a single fixed slot; this crate makes
//! the slot schedule explicit and testable. Ads arrive already auction-ordered
//! by eCPM (ad-rank), organics arrive rank-ordered, so mixing here never
//! re-scores anything — it only decides where commerce may appear and what
//! spills into the next page so pagination stays stable.

/// Slot depth policy: how many ads a page tolerates and how many organic
/// results must remain regardless of demand.
#[derive(Clone, Copy, Debug)]
pub struct MixPolicy {
    /// Ad load in basis points of the page length (1000 bps = 10%).
    pub load_bps: u16,
    /// Ads are suppressed entirely when fewer organic results than this exist,
    /// guaranteeing a useful, non-commercial experience during thin recalls.
    pub min_natural_results: usize,
}

impl MixPolicy {
    pub const fn new(load_bps: u16, min_natural_results: usize) -> Self {
        Self {
            load_bps,
            min_natural_results,
        }
    }

    /// Ad slots allowed for `page_len` organic-position page.
    pub fn ad_slots_for(&self, page_len: usize) -> usize {
        if page_len < self.min_natural_results {
            return 0;
        }
        let allowed = (page_len.saturating_mul(self.load_bps as usize)) / 10_000;
        // A deep page is always worth at most one more slot even under small loads.
        if self.load_bps > 0 && allowed == 0 && page_len >= 8 {
            1
        } else {
            // The mixer has a finite, explicitly audited slot schedule. Do
            // not reserve organic capacity for slots that cannot be rendered.
            allowed.min(DEFAULT_SLOT_FRACTIONS.len())
        }
    }
}

impl Default for MixPolicy {
    fn default() -> Self {
        Self::new(1_000, 3)
    }
}

/// Relative depth positions (fractions of the page) where ads compete. The
/// first slot sits in the upper quartile rather than the very top: the product
/// guarantees the opening experience stays purely organic.
pub const DEFAULT_SLOT_FRACTIONS: [f64; 4] = [0.25, 0.55, 0.75, 0.9];

/// One mixed item plus its provenance, so callers can enrich telemetry without
/// re-scanning types.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MixedItem<O, D> {
    Organic(O),
    Ad(D),
}

impl<O, D> MixedItem<O, D> {
    pub fn is_ad(&self) -> bool {
        matches!(self, MixedItem::Ad(_))
    }
}

/// Interleaves auction-ordered ads into rank-ordered organics.
///
/// Returns the combined page (length ≤ `limit`) and the displaced organic tail
/// that belongs to the *next* page's head, preserving cursor stability exactly
/// like the search surface's pending-buffer behavior.
///
/// Invariants:
/// - the first result position is always organic;
/// - ad count never exceeds `policy.ad_slots_for(limit)` nor the supplied ads;
/// - ads occupy increasing distinct positions and are consumed in eCPM order;
/// - all untouched organics keep their relative order;
/// - a page thinner than `min_natural_results` naturals stays commercial-free
///   instead of being padded with leftover inventory.
pub fn mix_page<O, D>(
    organic: Vec<O>,
    ads: Vec<D>,
    limit: usize,
    policy: MixPolicy,
) -> (Vec<MixedItem<O, D>>, Vec<O>) {
    let limit = limit.max(1);
    let supply = ads.len();
    // Depth targets into the ORIGINAL layout...
    let raw_targets = DEFAULT_SLOT_FRACTIONS
        .iter()
        .take(policy.ad_slots_for(limit))
        .map(|fraction| ((limit as f64) * fraction).round() as usize)
        .filter(|target| *target >= 1)
        .collect::<Vec<_>>();
    // ...then shift-adjusted onto the final (ads inserted) layout.
    let mut positions: Vec<usize> = Vec::new();
    for (offset, target) in raw_targets.into_iter().enumerate() {
        let candidate = (target + offset).min(limit - 1);
        if positions.last().is_none_or(|last| candidate > *last) {
            positions.push(candidate);
            if positions.len() == supply {
                break;
            }
        }
    }

    let mut mixed: Vec<MixedItem<O, D>> = Vec::with_capacity(positions.len().max(limit / 2));
    let mut ads_by_ecpm = ads.into_iter();
    let mut organics_left = organic.into_iter().peekable();
    let mut next_slot = 0usize;

    while mixed.len() < limit {
        // A thin recall ends before reaching any depth target, so pages with
        // fewer naturals than a slot's depth are commercial-free by geometry.
        if next_slot < positions.len() && mixed.len() == positions[next_slot] {
            match ads_by_ecpm.next() {
                Some(ad) => {
                    mixed.push(MixedItem::Ad(ad));
                    next_slot += 1;
                    continue;
                }
                None => break,
            }
        }
        match organics_left.next() {
            Some(item) => mixed.push(MixedItem::Organic(item)),
            // Naturals ran dry: end a thin page short rather than topping it
            // off with more commerce.
            None => break,
        }
    }

    (mixed, organics_left.collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn organics(n: usize) -> Vec<usize> {
        (0..n).collect()
    }

    fn ads(n: usize) -> Vec<&'static str> {
        (0..n).map(|i| match i % 4 {
            0 => "ad-a",
            1 => "ad-b",
            2 => "ad-c",
            _ => "ad-d",
        }).collect()
    }

    #[test]
    fn thin_organic_pages_stay_commercial_free() {
        let policy = MixPolicy::default(); // min 3 natural
        let (mixed, overflow) = mix_page(organics(2), ads(5), 20, policy);
        assert!(mixed.iter().all(|item| !item.is_ad()));
        assert!(overflow.is_empty());
        assert_eq!(mixed.len(), 2);
    }

    #[test]
    fn respects_load_cap_and_first_position_invariant() {
        let policy = MixPolicy::new(1_000, 3); // 10% of 20 => 2 slots
        let (mixed, _) = mix_page(organics(30), ads(5), 20, policy);
        let ad_count = mixed.iter().filter(|item| item.is_ad()).count();
        assert_eq!(ad_count, 2);
        assert!(!mixed[0].is_ad());
        let positions: Vec<usize> = mixed
            .iter()
            .enumerate()
            .filter_map(|(index, item)| item.is_ad().then_some(index))
            .collect();
        assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
        // Upper-quartile first slot keeps the opener organic and commercial density late-page.
        assert!(positions[0] >= 3, "first ad must sit below quarter depth, got {positions:?}");
    }

    #[test]
    fn ads_consume_auction_order_and_overflow_preserves_pagination() {
        let policy = MixPolicy::new(2_000, 1); // two slots at depths ~.25/.55
        let (mixed, overflow) = mix_page(
            organics(40),
            vec!["high-ecpm", "low-ecpm", "extra"],
            20,
            policy,
        );
        let ad_order: Vec<&&str> = mixed.iter().filter_map(|item| match item {
            MixedItem::Ad(ad) => Some(ad),
            MixedItem::Organic(_) => None,
        }).collect();
        // Supply of 3 ads fits within the 2000bps budget (=> 4 slot ceiling),
        // consumed strictly in eCPM order.
        assert_eq!(ad_order, vec![&"high-ecpm", &"low-ecpm", &"extra"]);
        assert_eq!(mixed.len(), 20);
        // Page holds 20 items of which 3 are ads, so exactly 17 organics were
        // consumed; displaced ones resume at index 17 with order intact.
        assert_eq!(overflow.len(), 23);
        assert_eq!(overflow.first(), Some(&17));
        for window in overflow.windows(2) {
            assert!(window[0] < window[1]);
        }
    }

    #[test]
    fn zero_load_disables_all_slots() {
        let policy = MixPolicy::new(0, 0);
        let (mixed, overflow) = mix_page(organics(10), ads(3), 10, policy);
        assert_eq!(mixed.len(), 10);
        assert!(overflow.is_empty());
        assert!(mixed.iter().all(|item| !item.is_ad()));
    }

    #[test]
    fn limit_floor_is_one_and_single_item_page_never_gets_ads() {
        let policy = MixPolicy::new(5_000, 1);
        let (mixed, overflow) = mix_page(vec!["only"], ads(2), 0, policy);
        assert_eq!(mixed.len(), 1);
        assert!(matches!(mixed[0], MixedItem::Organic("only")));
        assert!(overflow.is_empty());
    }

    #[test]
    fn duplicate_slot_targets_are_collapsed() {
        // Tiny page where several fraction targets round onto one index.
        let policy = MixPolicy::new(4_000, 2); // up to 4 slots demand
        let (mixed, _) = mix_page(organics(6), ads(6), 6, policy);
        let ad_count = mixed.iter().filter(|item| item.is_ad()).count();
        assert!(ad_count <= 2, "small page must collapse duplicate targets, got {ad_count}");
    }

    #[test]
    fn slot_demand_never_exceeds_the_renderable_schedule() {
        let policy = MixPolicy::new(10_000, 1);
        assert_eq!(policy.ad_slots_for(100), DEFAULT_SLOT_FRACTIONS.len());

        let (mixed, overflow) = mix_page(organics(96), ads(10), 100, policy);
        assert_eq!(mixed.len(), 100);
        assert_eq!(mixed.iter().filter(|item| item.is_ad()).count(), 4);
        assert!(overflow.is_empty());
    }
}
