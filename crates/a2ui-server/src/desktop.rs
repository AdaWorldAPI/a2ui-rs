//! `desktop` — the **live session orchestration** (charter C2 service shape +
//! C1.5/C1.6), tying the W2 projection + the W5 sealed transport into one
//! "Citrix without pixels" loop.
//!
//! The A2UI fork's `hamming.proto` service shape is kept — `RenderStream`
//! (server → client surface), `ActionStream` (client → server actions),
//! `SyncCodebook` (template/codebook amortization) — while the payload is the
//! V3 facet (charter C2). [`DesktopSession`] is that shape as ONE object:
//!
//! - [`render_node`](DesktopSession::render_node) — the **RenderStream**:
//!   RBAC-project a node (W2) → seal the `NodeDelta` down (W5). Returns the
//!   sealed wire bytes + the askama fieldview surface (the membrane render of
//!   the same projection).
//! - [`receive_action`](DesktopSession::receive_action) — the **ActionStream**:
//!   open a sealed `ActionInvoke` (W5) → resolve its ordinal to an `ActionDef`
//!   by address (W2, trap T2) → record a **Klickweg edge** (C1.6: a click IS a
//!   `navigates_to`/`ActionInvocation` edge — interaction telemetry as graph
//!   edges, zero new vocabulary).
//! - [`sync_codebook`](DesktopSession::sync_codebook) — the **SyncCodebook**:
//!   hand the client the template codebook (the *font of the desktop*) + a
//!   version, so the ClassView/templates amortize once (D-AMORT).
//!
//! The session is a **capability over classid ranges + a role mask** (charter
//! C1.5); the transport is keyed by the Argon2id session key. Both directions
//! are separate [`SealedTransport`](crate::transport::SealedTransport)s so the
//! two half-duplex streams never share a `(key, nonce)`.

use std::collections::HashMap;

use lance_graph_contract::class_view::{ClassId, ClassView};

use crate::action_stream::{
    ActionError, ActionSpec, ResolvedAction, concept_of_key, resolve_action,
};
use crate::render_stream::{ProjectionError, project_node};
use crate::session::{Session, SessionError};
use crate::transport::{Direction, SealedTransport, TransportError};
use crate::{Frame, KdfParams, WideFieldMask};

/// One class registered in the session's codebook: its concept name + its
/// ordinal-ordered `ActionDef` set (the runtime projection of the ruff-codegen'd
/// DO-arm). Used for both action resolution and [`sync_codebook`](DesktopSession::sync_codebook).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassEntry {
    /// The canonical concept name.
    pub concept: String,
    /// The class's actions, in ordinal order (index = `ActionInvoke` ordinal).
    pub actions: Vec<ActionSpec>,
}

/// One interaction edge — the runtime form of a Klickweg (charter C1.6: "a
/// click IS a `navigates_to`/`ActionInvocation` edge"). The SAME closed
/// vocabulary the C# harvest emits statically becomes this runtime event
/// stream, landing as SPO-shaped graph telemetry with zero new vocabulary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KlickwegEdge {
    /// Subject — the node the click happened on (its canonical GUID).
    pub from_key: [u8; 16],
    /// The concept the key resolved to.
    pub class_id: ClassId,
    /// The action's ordinal address within the class.
    pub ordinal: u32,
    /// Predicate — the `ActionDef` predicate the ordinal named (the edge label).
    pub predicate: String,
    /// A monotonic per-session event sequence (which action, in order).
    pub seq: u64,
}

/// The amortization payload for one class — what a client caches so it never
/// re-fetches the template (the *font of the desktop*).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassCodebook {
    /// The class discriminator.
    pub class_id: ClassId,
    /// The canonical concept name (the surface's `data-concept`).
    pub concept: String,
    /// Ordinal-ordered action captions the client renders as controls.
    pub action_labels: Vec<String>,
}

/// A versioned snapshot of the session's template codebook — the `SyncCodebook`
/// RPC payload. The client caches it; a later `PushCodebookDelta` bumps the
/// version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodebookSnapshot {
    /// The codebook version the client should record.
    pub version: u32,
    /// One entry per registered class.
    pub classes: Vec<ClassCodebook>,
}

/// The sealed down-wire frame + its membrane render — the output of
/// [`render_node`](DesktopSession::render_node).
#[derive(Debug, Clone)]
pub struct RenderedFrame {
    /// The sealed `NodeDelta` — the bytes that go down the wire (W5).
    pub sealed: Vec<u8>,
    /// The askama fieldview surface — the membrane/debug render of the same
    /// projection (a non-wasm client can paint this directly).
    pub html: String,
}

/// Why a desktop-session operation failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DesktopError {
    /// Session establishment failed (KDF / range / empty-role).
    Session(SessionError),
    /// The class is not registered in the session codebook.
    ClassNotRegistered(ClassId),
    /// The RBAC projection / render failed (W2).
    Projection(ProjectionError),
    /// The sealed transport failed (W5).
    Transport(TransportError),
    /// A sealed up-frame was expected to be an `ActionInvoke` but was not.
    NotAnAction,
    /// The action's ordinal did not resolve (W2).
    Action(ActionError),
}

impl core::fmt::Display for DesktopError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Session(e) => write!(f, "session: {e}"),
            Self::ClassNotRegistered(c) => write!(f, "class 0x{c:04X} not registered in codebook"),
            Self::Projection(e) => write!(f, "projection: {e}"),
            Self::Transport(e) => write!(f, "transport: {e}"),
            Self::NotAnAction => f.write_str("expected an ActionInvoke up the wire"),
            Self::Action(e) => write!(f, "action: {e}"),
        }
    }
}

impl std::error::Error for DesktopError {}

/// A live rdp-2graph desktop session — the C2 service shape as one object.
pub struct DesktopSession {
    session: Session,
    down: SealedTransport,
    up: SealedTransport,
    codebook: HashMap<ClassId, ClassEntry>,
    codebook_version: u32,
    klickwege: Vec<KlickwegEdge>,
    event_seq: u64,
}

impl DesktopSession {
    /// Establish a desktop session — derive the session key (Argon2id, once),
    /// and open the two sealed half-duplex transports keyed by it. The
    /// codebook starts empty; register classes before rendering.
    ///
    /// **`salt` MUST be a fresh CSPRNG draw per session** — the whole transport
    /// depends on the key being unique (see the security note on
    /// [`Session::establish`]). Prefer
    /// [`establish_random_salt`](DesktopSession::establish_random_salt); this
    /// raw entry point is for the peer side of a handshake (which receives the
    /// salt) and for deterministic tests.
    ///
    /// # Errors
    ///
    /// [`DesktopError::Session`] if the underlying [`Session::establish`] fails
    /// (KDF, inverted range, or empty role mask).
    pub fn establish(
        secret: &[u8],
        salt: &[u8; 16],
        role_mask: WideFieldMask,
        class_range: (ClassId, ClassId),
        params: &KdfParams,
        codebook_version: u32,
    ) -> Result<Self, DesktopError> {
        let session = Session::establish(secret, salt, role_mask, class_range, params)
            .map_err(DesktopError::Session)?;
        Ok(Self::from_session(session, codebook_version))
    }

    /// Establish a desktop session with a **fresh CSPRNG salt** — the safe path
    /// (see [`Session::establish_random_salt`]). Returns `(session, salt)`; hand
    /// the public `salt` to the peer so it derives the same key. Two sessions
    /// never share a key, so `(key, nonce)` pairs never collide.
    ///
    /// # Errors
    ///
    /// [`DesktopError::Session`] (incl. [`SessionError::Rng`] if the platform
    /// CSPRNG is unavailable).
    pub fn establish_random_salt(
        secret: &[u8],
        role_mask: WideFieldMask,
        class_range: (ClassId, ClassId),
        params: &KdfParams,
        codebook_version: u32,
    ) -> Result<(Self, [u8; 16]), DesktopError> {
        let (session, salt) =
            Session::establish_random_salt(secret, role_mask, class_range, params)
                .map_err(DesktopError::Session)?;
        Ok((Self::from_session(session, codebook_version), salt))
    }

    /// Build the session object from an already-derived [`Session`] — the shared
    /// core of both establishment paths (opens the two sealed transports).
    fn from_session(session: Session, codebook_version: u32) -> Self {
        let key = session.key();
        Self {
            session,
            down: SealedTransport::new(key, Direction::ServerToClient),
            up: SealedTransport::new(key, Direction::ClientToServer),
            codebook: HashMap::new(),
            codebook_version,
            klickwege: Vec::new(),
            event_seq: 0,
        }
    }

    /// Register (or replace) a class in the session codebook. The `actions` are
    /// the class's `ActionDef` set in ordinal order (resolution + sync use it).
    pub fn register_class(
        &mut self,
        class_id: ClassId,
        concept: impl Into<String>,
        actions: Vec<ActionSpec>,
    ) {
        self.codebook.insert(
            class_id,
            ClassEntry {
                concept: concept.into(),
                actions,
            },
        );
    }

    /// The 32-byte session key (hand to a client-side [`SealedTransport`]).
    #[must_use]
    pub fn session_key(&self) -> [u8; 32] {
        self.session.key()
    }

    /// **RenderStream:** RBAC-project one node and seal it down the wire.
    ///
    /// Resolves the class from the key (`concept_of_key`), projects the node
    /// (W2 — `surface ∩ role`, fail-closed), seals the resulting `NodeDelta`
    /// (W5), and returns the sealed bytes + the askama fieldview surface.
    ///
    /// # Errors
    ///
    /// [`DesktopError::ClassNotRegistered`] if the key's concept has no codebook
    /// entry; [`DesktopError::Projection`] if the RBAC projection / render is
    /// refused.
    pub fn render_node<V: ClassView>(
        &mut self,
        view: &V,
        key: [u8; 16],
        facet: &[u8; 12],
        title: &str,
        fmt: impl FnMut(u8, u8) -> String,
    ) -> Result<RenderedFrame, DesktopError> {
        let class_id = concept_of_key(&key);
        let entry = self
            .codebook
            .get(&class_id)
            .ok_or(DesktopError::ClassNotRegistered(class_id))?;
        let projection = project_node(
            &self.session,
            view,
            &entry.concept,
            key,
            facet,
            &entry.actions,
            title,
            fmt,
        )
        .map_err(DesktopError::Projection)?;
        let sealed = self.down.seal_frame(&Frame::NodeDelta(projection.delta));
        Ok(RenderedFrame {
            sealed,
            html: projection.html,
        })
    }

    /// **ActionStream:** open a sealed up-frame, resolve its ordinal to an
    /// `ActionDef` by address, and record the Klickweg edge.
    ///
    /// # Errors
    ///
    /// [`DesktopError::Transport`] if the sealed frame fails to open (wrong
    /// key / tamper / replay); [`DesktopError::NotAnAction`] if it decodes to a
    /// `NodeDelta`; [`DesktopError::ClassNotRegistered`] if the key's concept
    /// has no codebook entry; [`DesktopError::Action`] if the ordinal doesn't
    /// resolve (out of range or unauthorized class).
    pub fn receive_action(&mut self, sealed: &[u8]) -> Result<ResolvedAction, DesktopError> {
        let frame = self
            .up
            .open_frame(sealed)
            .map_err(DesktopError::Transport)?;
        let invoke = match frame {
            Frame::ActionInvoke(a) => a,
            Frame::NodeDelta(_) => return Err(DesktopError::NotAnAction),
        };
        let class_id = concept_of_key(&invoke.key);
        let entry = self
            .codebook
            .get(&class_id)
            .ok_or(DesktopError::ClassNotRegistered(class_id))?;
        let resolved =
            resolve_action(&self.session, &invoke, &entry.actions).map_err(DesktopError::Action)?;

        // C1.6 — the click IS an edge. Record the interaction as SPO telemetry.
        self.klickwege.push(KlickwegEdge {
            from_key: resolved.key,
            class_id: resolved.class_id,
            ordinal: resolved.ordinal,
            predicate: resolved.predicate.clone(),
            seq: self.event_seq,
        });
        self.event_seq += 1;
        Ok(resolved)
    }

    /// **SyncCodebook:** a versioned snapshot of the template codebook — the
    /// amortization payload the client caches (the *font of the desktop*).
    #[must_use]
    pub fn sync_codebook(&self) -> CodebookSnapshot {
        let mut classes: Vec<ClassCodebook> = self
            .codebook
            .iter()
            .map(|(&class_id, entry)| ClassCodebook {
                class_id,
                concept: entry.concept.clone(),
                action_labels: entry.actions.iter().map(|a| a.label.clone()).collect(),
            })
            .collect();
        // Deterministic order (HashMap iteration is unordered) so the snapshot
        // is stable/comparable across calls.
        classes.sort_by_key(|c| c.class_id);
        CodebookSnapshot {
            version: self.codebook_version,
            classes,
        }
    }

    /// The current codebook version.
    #[must_use]
    pub fn codebook_version(&self) -> u32 {
        self.codebook_version
    }

    /// The accumulated Klickweg edges — the runtime interaction-telemetry graph
    /// (C1.6). Append-only within a session; drains via [`take_klickwege`](Self::take_klickwege).
    #[must_use]
    pub fn klickwege(&self) -> &[KlickwegEdge] {
        &self.klickwege
    }

    /// Drain the accumulated Klickweg edges (e.g. to flush them into the graph),
    /// leaving the session's log empty.
    pub fn take_klickwege(&mut self) -> Vec<KlickwegEdge> {
        std::mem::take(&mut self.klickwege)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::SealedTransport;
    use a2ui_core::{ActionInvoke, Frame as CoreFrame};
    use lance_graph_contract::class_view::ClassId as CvClassId;
    use lance_graph_contract::ontology::{DisplayTemplate, FieldRef};

    const FAST: KdfParams = KdfParams {
        m_cost_kib: 32,
        t_cost: 1,
        p_cost: 1,
    };

    struct DemoView {
        fields: Vec<FieldRef>,
    }
    impl DemoView {
        fn new() -> Self {
            Self {
                fields: vec![
                    FieldRef::new("amount_total", "Total"),
                    FieldRef::new("state", "Status"),
                    FieldRef::new("partner_id", "Partner"),
                ],
            }
        }
    }
    impl ClassView for DemoView {
        fn fields(&self, _c: CvClassId) -> &[FieldRef] {
            &self.fields
        }
        fn template(&self, _c: CvClassId) -> DisplayTemplate {
            DisplayTemplate::Detail
        }
        fn dolce_category_id(&self, _c: CvClassId) -> u8 {
            0
        }
    }

    fn key_0102() -> [u8; 16] {
        let classid: u32 = (0x0102u32 << 16) | 0x0009;
        let mut k = [0u8; 16];
        k[0..4].copy_from_slice(&classid.to_le_bytes());
        k[10..16].copy_from_slice(&[1, 2, 3, 4, 5, 6]);
        k
    }

    fn server() -> DesktopSession {
        let mut s = DesktopSession::establish(
            b"shared-secret",
            &[9; 16],
            WideFieldMask::from_positions(&[0, 1, 2]),
            (0x0100, 0x01FF),
            &FAST,
            7,
        )
        .unwrap();
        s.register_class(
            0x0102,
            "commercial_document",
            vec![
                ActionSpec::new("view", "View"),
                ActionSpec::new("action_post", "Post"),
            ],
        );
        s
    }

    #[test]
    fn render_stream_seals_a_projection_the_client_can_open() {
        let mut srv = server();
        let view = DemoView::new();
        let key = key_0102();
        let facet = [10u8, 20, 30, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let rendered = srv
            .render_node(&view, key, &facet, "Invoice", |_, b| b.to_string())
            .unwrap();

        // The membrane render is the addressed surface.
        assert!(rendered.html.contains("data-field-pos=\"0\""));
        assert!(rendered.html.contains("data-action-ordinal=\"1\""));

        // A paired client transport opens the sealed NodeDelta.
        let mut client_down = SealedTransport::new(srv.session_key(), Direction::ServerToClient);
        match client_down.open_frame(&rendered.sealed).unwrap() {
            CoreFrame::NodeDelta(nd) => {
                assert_eq!(nd.key, key);
                assert_eq!(nd.values, vec![10, 20, 30]);
            }
            CoreFrame::ActionInvoke(_) => panic!("expected NodeDelta"),
        }
    }

    #[test]
    fn action_stream_resolves_and_records_a_klickweg_edge() {
        let mut srv = server();
        let key = key_0102();
        // The client seals an ActionInvoke (ordinal 1 = "Post") up the wire.
        let mut client_up = SealedTransport::new(srv.session_key(), Direction::ClientToServer);
        let sealed = client_up.seal_frame(&CoreFrame::ActionInvoke(ActionInvoke {
            key,
            action_ordinal: 1,
            args: vec![],
        }));

        let resolved = srv.receive_action(&sealed).unwrap();
        assert_eq!(resolved.predicate, "action_post");

        // The click landed as a Klickweg edge (C1.6).
        let edges = srv.klickwege();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].from_key, key);
        assert_eq!(edges[0].class_id, 0x0102);
        assert_eq!(edges[0].ordinal, 1);
        assert_eq!(edges[0].predicate, "action_post");
        assert_eq!(edges[0].seq, 0);

        // A second click advances the sequence; take_klickwege drains.
        let sealed2 = client_up.seal_frame(&CoreFrame::ActionInvoke(ActionInvoke {
            key,
            action_ordinal: 0,
            args: vec![],
        }));
        srv.receive_action(&sealed2).unwrap();
        let drained = srv.take_klickwege();
        assert_eq!(drained.len(), 2);
        assert_eq!(drained[1].predicate, "view");
        assert_eq!(drained[1].seq, 1);
        assert!(srv.klickwege().is_empty(), "drained");
    }

    #[test]
    fn sync_codebook_is_versioned_and_deterministic() {
        let mut srv = server();
        srv.register_class(0x0100, "patient", vec![ActionSpec::new("admit", "Admit")]);
        let snap = srv.sync_codebook();
        assert_eq!(snap.version, 7);
        // Sorted by class_id → 0x0100 before 0x0102.
        assert_eq!(snap.classes[0].class_id, 0x0100);
        assert_eq!(snap.classes[0].concept, "patient");
        assert_eq!(snap.classes[1].class_id, 0x0102);
        assert_eq!(snap.classes[1].action_labels, vec!["View", "Post"]);
    }

    #[test]
    fn tampered_up_frame_is_refused_no_klickweg_recorded() {
        let mut srv = server();
        let mut client_up = SealedTransport::new(srv.session_key(), Direction::ClientToServer);
        let mut sealed = client_up.seal_frame(&CoreFrame::ActionInvoke(ActionInvoke {
            key: key_0102(),
            action_ordinal: 0,
            args: vec![],
        }));
        let n = sealed.len();
        sealed[n - 1] ^= 0x01; // flip a tag bit
        assert!(matches!(
            srv.receive_action(&sealed),
            Err(DesktopError::Transport(_))
        ));
        assert!(srv.klickwege().is_empty(), "no edge on a refused frame");
    }

    #[test]
    fn establish_random_salt_yields_a_working_session_and_a_fresh_salt() {
        let (mut a, salt_a) = DesktopSession::establish_random_salt(
            b"secret",
            WideFieldMask::from_positions(&[0, 1, 2]),
            (0x0100, 0x01FF),
            &FAST,
            1,
        )
        .unwrap();
        let (b, salt_b) = DesktopSession::establish_random_salt(
            b"secret",
            WideFieldMask::from_positions(&[0, 1, 2]),
            (0x0100, 0x01FF),
            &FAST,
            1,
        )
        .unwrap();
        // Fresh salt per call → distinct session keys (no nonce collision).
        assert_ne!(salt_a, salt_b);
        assert_ne!(a.session_key(), b.session_key());
        // And the session renders end-to-end.
        a.register_class(
            0x0102,
            "commercial_document",
            vec![ActionSpec::new("view", "V")],
        );
        let view = DemoView::new();
        let rendered = a
            .render_node(&view, key_0102(), &[1u8; 12], "T", |_, b| b.to_string())
            .unwrap();
        let mut client = SealedTransport::new(a.session_key(), Direction::ServerToClient);
        assert!(client.open_frame(&rendered.sealed).is_ok());
    }

    #[test]
    fn unregistered_class_render_is_refused() {
        let mut srv = DesktopSession::establish(
            b"s",
            &[1; 16],
            WideFieldMask::from_positions(&[0]),
            (0x0100, 0x01FF),
            &FAST,
            1,
        )
        .unwrap();
        // no register_class → codebook empty
        let view = DemoView::new();
        let r = srv.render_node(&view, key_0102(), &[0u8; 12], "T", |_, b| b.to_string());
        assert_eq!(r.err(), Some(DesktopError::ClassNotRegistered(0x0102)));
    }
}
