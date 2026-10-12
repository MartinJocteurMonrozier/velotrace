//! Point de contrôle FTMS : limitation du débit des consignes et lecture
//! des réponses du home trainer.

use std::time::{Duration, Instant};

use velo_core::protocol::ControlCommand;

/// Intervalle minimal entre deux consignes : au plus 4 par seconde.
pub const SETPOINT_INTERVAL: Duration = Duration::from_millis(250);

/// Opcode FTMS de la prise de contrôle.
pub const OP_REQUEST_CONTROL: u8 = 0x00;
/// Opcode FTMS du démarrage ou de la reprise.
pub const OP_START_OR_RESUME: u8 = 0x07;

/// Consigne de résistance (pente ou puissance cible), par opposition aux
/// commandes de gestion (prise de contrôle, remise à zéro, démarrage).
pub fn is_setpoint(cmd: &ControlCommand) -> bool {
    matches!(
        cmd,
        ControlCommand::Simulation { .. } | ControlCommand::TargetPower(_)
    )
}

/// Limite le débit des consignes envoyées au trainer.
///
/// L'application peut envoyer une pente à chaque image : seule la plus
/// récente compte. Elle part au plus tous les `interval`, et jamais si elle
/// est identique, une fois encodée, à la dernière envoyée.
#[derive(Debug, Clone)]
pub struct CommandThrottle {
    interval: Duration,
    pending: Option<ControlCommand>,
    last: Option<ControlCommand>,
    last_at: Option<Instant>,
}

impl Default for CommandThrottle {
    fn default() -> Self {
        Self::new(SETPOINT_INTERVAL)
    }
}

impl CommandThrottle {
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            pending: None,
            last: None,
            last_at: None,
        }
    }

    /// Nouvelle consigne : remplace celle en attente.
    pub fn submit(&mut self, cmd: ControlCommand) {
        self.pending = Some(cmd);
    }

    /// Consigne à envoyer maintenant, s'il y en a une.
    pub fn due(&mut self, now: Instant) -> Option<ControlCommand> {
        let cmd = self.pending?;
        if self.last.is_some_and(|l| l.encode() == cmd.encode()) {
            // Rien de neuf pour le trainer.
            self.pending = None;
            return None;
        }
        if self
            .last_at
            .is_some_and(|t| now.duration_since(t) < self.interval)
        {
            return None;
        }
        self.pending = None;
        self.last = Some(cmd);
        self.last_at = Some(now);
        Some(cmd)
    }

    /// Fait repartir la dernière consigne, par exemple après une perte du
    /// contrôle ou une reconnexion.
    pub fn resend(&mut self) {
        if self.pending.is_none() {
            self.pending = self.last;
        }
        self.last = None;
    }

    /// Oublie tout (après une remise à zéro du trainer).
    pub fn clear(&mut self) {
        self.pending = None;
        self.last = None;
    }

    /// Dernière consigne connue, envoyée ou non.
    pub fn latest(&self) -> Option<ControlCommand> {
        self.pending.or(self.last)
    }
}

/// Code résultat d'une réponse du point de contrôle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlResult {
    Success,
    NotSupported,
    InvalidParameter,
    Failed,
    ControlNotPermitted,
    Unknown(u8),
}

impl ControlResult {
    pub fn label(self) -> &'static str {
        match self {
            Self::Success => "OK",
            Self::NotSupported => "non pris en charge",
            Self::InvalidParameter => "paramètre invalide",
            Self::Failed => "échec",
            Self::ControlNotPermitted => "contrôle non accordé",
            Self::Unknown(_) => "code inconnu",
        }
    }
}

/// Réponse du trainer à une commande.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControlResponse {
    /// Opcode de la requête à laquelle le trainer répond.
    pub request: u8,
    pub result: ControlResult,
}

/// Décode une indication du point de contrôle : 0x80, opcode, résultat.
pub fn parse_control_response(data: &[u8]) -> Option<ControlResponse> {
    let &[0x80, request, code, ..] = data else {
        return None;
    };
    let result = match code {
        0x01 => ControlResult::Success,
        0x02 => ControlResult::NotSupported,
        0x03 => ControlResult::InvalidParameter,
        0x04 => ControlResult::Failed,
        0x05 => ControlResult::ControlNotPermitted,
        c => ControlResult::Unknown(c),
    };
    Some(ControlResponse { request, result })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grade(g: f64) -> ControlCommand {
        ControlCommand::Simulation {
            wind_ms: 0.0,
            grade: g,
            crr: 0.004,
            cw_kg_m: 0.51,
        }
    }

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn setpoints_are_recognized() {
        assert!(is_setpoint(&grade(0.02)));
        assert!(is_setpoint(&ControlCommand::TargetPower(200)));
        assert!(!is_setpoint(&ControlCommand::RequestControl));
        assert!(!is_setpoint(&ControlCommand::Reset));
        assert!(!is_setpoint(&ControlCommand::StartOrResume));
    }

    #[test]
    fn throttle_keeps_only_the_latest() {
        let t0 = Instant::now();
        let mut t = CommandThrottle::default();
        assert_eq!(t.due(t0), None);
        t.submit(grade(0.01));
        assert_eq!(t.due(t0), Some(grade(0.01)));
        // 60 images par seconde : seule la dernière part, 250 ms plus tard.
        for i in 0..15 {
            t.submit(grade(0.01 + i as f64 * 0.001));
            assert_eq!(t.due(t0 + ms(i * 16)), None);
        }
        assert_eq!(t.due(t0 + ms(249)), None);
        assert_eq!(
            t.due(t0 + ms(250)).map(|c| c.encode()),
            Some(grade(0.024).encode())
        );
        assert_eq!(t.due(t0 + ms(600)), None);
    }

    #[test]
    fn identical_setpoints_are_not_resent() {
        let t0 = Instant::now();
        let mut t = CommandThrottle::default();
        t.submit(grade(0.05));
        assert!(t.due(t0).is_some());
        // Même trame une fois encodée (0,01 % près) : rien n'est envoyé.
        t.submit(grade(0.050_001));
        assert_eq!(t.due(t0 + ms(1000)), None);
        // Un changement repart aussitôt l'intervalle écoulé.
        t.submit(grade(0.06));
        assert_eq!(t.due(t0 + ms(1000)), Some(grade(0.06)));
        // Passage en mode ERG.
        t.submit(ControlCommand::TargetPower(180));
        assert_eq!(t.due(t0 + ms(1100)), None);
        assert_eq!(t.due(t0 + ms(1250)), Some(ControlCommand::TargetPower(180)));
    }

    #[test]
    fn resend_and_clear() {
        let t0 = Instant::now();
        let mut t = CommandThrottle::default();
        t.submit(grade(0.03));
        t.due(t0);
        t.resend();
        assert_eq!(t.latest(), Some(grade(0.03)));
        assert_eq!(t.due(t0 + ms(300)), Some(grade(0.03)));
        // Une consigne en attente n'est pas écrasée par la précédente.
        t.submit(grade(0.04));
        t.resend();
        assert_eq!(t.due(t0 + ms(600)), Some(grade(0.04)));
        t.clear();
        assert_eq!(t.latest(), None);
        assert_eq!(t.due(t0 + ms(900)), None);
        // Après une remise à zéro, la même pente repart.
        t.submit(grade(0.04));
        assert_eq!(t.due(t0 + ms(1200)), Some(grade(0.04)));
    }

    #[test]
    fn control_point_responses() {
        assert_eq!(
            parse_control_response(&[0x80, 0x00, 0x01]),
            Some(ControlResponse {
                request: OP_REQUEST_CONTROL,
                result: ControlResult::Success
            })
        );
        let refused = parse_control_response(&[0x80, 0x11, 0x05]).unwrap();
        assert_eq!(refused.request, 0x11);
        assert_eq!(refused.result, ControlResult::ControlNotPermitted);
        assert_eq!(refused.result.label(), "contrôle non accordé");
        assert_eq!(
            parse_control_response(&[0x80, 0x05, 0x09]).unwrap().result,
            ControlResult::Unknown(9)
        );
        assert_eq!(parse_control_response(&[0x80, 0x00]), None);
        assert_eq!(parse_control_response(&[0x11, 0x00, 0x01]), None);
    }
}
