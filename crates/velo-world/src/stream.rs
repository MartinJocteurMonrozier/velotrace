//! Génération des carrés proches à la demande, par des fils de travail.

use crate::world::{ChunkDetail, World};
use std::collections::{HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;

#[derive(Default)]
struct Shared {
    queue: Mutex<VecDeque<usize>>,
    wake: Condvar,
    /// Carrés en cours de génération.
    busy: Mutex<HashSet<usize>>,
    done: Mutex<Vec<ChunkDetail>>,
    stop: AtomicBool,
}

pub struct Streamer {
    world: Arc<World>,
    shared: Arc<Shared>,
    threads: Vec<JoinHandle<()>>,
}

impl Streamer {
    pub fn new(world: Arc<World>, threads: usize) -> Self {
        let shared = Arc::new(Shared::default());
        let threads = (0..threads.max(1))
            .map(|_| {
                let (world, shared) = (world.clone(), shared.clone());
                std::thread::spawn(move || loop {
                    let index = {
                        let mut q = shared.queue.lock().unwrap();
                        loop {
                            if shared.stop.load(Ordering::Relaxed) {
                                return;
                            }
                            if let Some(i) = q.pop_front() {
                                shared.busy.lock().unwrap().insert(i);
                                break i;
                            }
                            q = shared.wake.wait(q).unwrap();
                        }
                    };
                    let detail = world.near_detail(index);
                    shared.done.lock().unwrap().push(detail);
                    shared.busy.lock().unwrap().remove(&index);
                })
            })
            .collect();
        Self {
            world,
            shared,
            threads,
        }
    }

    pub fn world(&self) -> &Arc<World> {
        &self.world
    }

    /// Remplace la file d'attente par ces carrés, du plus urgent au moins
    /// urgent. Ceux déjà en cours ou prêts ne sont pas refaits.
    pub fn want(&self, indices: &[usize]) {
        let busy = self.shared.busy.lock().unwrap().clone();
        let ready: HashSet<usize> = self
            .shared
            .done
            .lock()
            .unwrap()
            .iter()
            .map(|d| d.index)
            .collect();
        let mut q = self.shared.queue.lock().unwrap();
        q.clear();
        q.extend(
            indices
                .iter()
                .copied()
                .filter(|i| *i < self.world.near.len() && !busy.contains(i) && !ready.contains(i)),
        );
        drop(q);
        self.shared.wake.notify_all();
    }

    /// Carrés prêts depuis le dernier appel (au plus `max`).
    pub fn take(&self, max: usize) -> Vec<ChunkDetail> {
        let mut done = self.shared.done.lock().unwrap();
        let n = done.len().min(max);
        done.drain(..n).collect()
    }

    /// Nombre de carrés en attente ou en cours.
    pub fn pending(&self) -> usize {
        self.shared.queue.lock().unwrap().len() + self.shared.busy.lock().unwrap().len()
    }
}

impl Drop for Streamer {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        self.shared.wake.notify_all();
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}
