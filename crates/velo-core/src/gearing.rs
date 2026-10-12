//! Transmission et changement de vitesse virtuel (Zwift Click).

use crate::config::Bike;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gear {
    pub chainring: u32,
    pub cog: u32,
}

impl Gear {
    pub fn ratio(&self) -> f64 {
        self.chainring as f64 / self.cog as f64
    }

    /// Développement : mètres parcourus par tour de pédale.
    pub fn development_m(&self, wheel_circumference_m: f64) -> f64 {
        self.ratio() * wheel_circumference_m
    }
}

/// Suite ordonnée de rapports parcourue par les boutons + et −.
///
/// Comme le « synchro shift » Shimano : on monte sur le petit plateau jusqu'à
/// ce que le grand plateau donne un rapport plus dur, puis on bascule. Les
/// rapports croisés extrêmes sont ainsi évités et chaque clic durcit
/// réellement le braquet.
#[derive(Debug, Clone)]
pub struct VirtualGearbox {
    gears: Vec<Gear>,
    index: usize,
}

impl VirtualGearbox {
    pub fn synchro(bike: &Bike) -> Self {
        let mut rings = bike.chainrings.clone();
        rings.sort_unstable();
        let mut cogs = bike.cassette.clone();
        cogs.sort_unstable_by(|a, b| b.cmp(a)); // du plus facile au plus dur

        // Sur les cassettes de 9 pignons et plus, on évite les deux pignons
        // extrêmes en croisement de chaîne (petit plateau sur les petits
        // pignons, grand plateau sur les grands).
        let cross = if cogs.len() >= 9 { 2 } else { 0 };
        let ladder = |ring: u32, skip_easy: usize, skip_hard: usize| -> Vec<Gear> {
            cogs[skip_easy..cogs.len() - skip_hard]
                .iter()
                .map(|&cog| Gear {
                    chainring: ring,
                    cog,
                })
                .collect()
        };

        let last = rings.len() - 1;
        let mut gears = ladder(rings[0], 0, if last > 0 { cross } else { 0 });
        for (i, &ring) in rings.iter().enumerate().skip(1) {
            let next = ladder(ring, cross, if i < last { cross } else { 0 });
            // Point de bascule : celui où le changement de plateau fait le
            // plus petit saut de braquet.
            let (keep, _) = (1..=gears.len())
                .filter_map(|keep| {
                    let top = gears[keep - 1].ratio();
                    next.iter()
                        .find(|g| g.ratio() > top)
                        .map(|g| (keep, g.ratio() / top))
                })
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .expect("au moins un rapport plus dur sur le plateau suivant");
            let top = gears[keep - 1].ratio();
            gears.truncate(keep);
            gears.extend(next.into_iter().filter(|g| g.ratio() > top));
        }
        let index = gears.len() / 2;
        Self { gears, index }
    }

    pub fn gears(&self) -> &[Gear] {
        &self.gears
    }

    pub fn current(&self) -> Gear {
        self.gears[self.index]
    }

    pub fn index(&self) -> usize {
        self.index
    }

    pub fn set_index(&mut self, i: usize) {
        self.index = i.min(self.gears.len() - 1);
    }

    /// Bouton + : rapport plus dur. Renvoie `false` en butée.
    pub fn shift_up(&mut self) -> bool {
        if self.index + 1 < self.gears.len() {
            self.index += 1;
            true
        } else {
            false
        }
    }

    /// Bouton − : rapport plus souple.
    pub fn shift_down(&mut self) -> bool {
        if self.index > 0 {
            self.index -= 1;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Profile;

    #[test]
    fn martin_synchro_avoids_cross_chain() {
        let gb = VirtualGearbox::synchro(&Profile::martin().bike);
        let g = gb.gears();
        assert_eq!(g.len(), 16);
        assert!(g.iter().all(|g| !(g.chainring == 36 && g.cog <= 13)));
        assert!(g.iter().all(|g| !(g.chainring == 52 && g.cog >= 30)));
    }

    #[test]
    fn martin_synchro_sequence_is_strictly_harder() {
        let gb = VirtualGearbox::synchro(&Profile::martin().bike);
        let g = gb.gears();
        assert_eq!(
            g.first().copied(),
            Some(Gear {
                chainring: 36,
                cog: 34
            })
        );
        assert_eq!(
            g.last().copied(),
            Some(Gear {
                chainring: 52,
                cog: 11
            })
        );
        for w in g.windows(2) {
            assert!(w[1].ratio() > w[0].ratio(), "{:?} -> {:?}", w[0], w[1]);
            // Pas de saut de plus de 20 % entre deux clics.
            assert!(
                w[1].ratio() / w[0].ratio() < 1.2,
                "{:?} -> {:?}",
                w[0],
                w[1]
            );
        }
    }

    #[test]
    fn shifting_stops_at_ends() {
        let mut gb = VirtualGearbox::synchro(&Profile::martin().bike);
        gb.set_index(0);
        assert!(!gb.shift_down());
        gb.set_index(usize::MAX);
        assert!(!gb.shift_up());
        assert!(gb.shift_down());
    }
}
