//! Containers and services, as the reference's `parseCredentials`,
//! `parseContainer`, and `parseServices` at `parse.go:1007-1110`.
//!
//! A job's `container` and each service of `services` are containers. A
//! container is an image string or a mapping with case-sensitive keys, and
//! only a service may have `command` and `entrypoint`. Service IDs are
//! case-insensitive and keep their source order.

use workflint_diagnostics::quote::quote;
use workflint_yaml::{Node, NodeKind};

use super::mapping::Key;
use super::shared::mapping_field;
use super::{KeySpec, Label, MapEntry, Parser, Section};
use crate::rules::{INCOMPLETE_CREDENTIALS, MISSING_REQUIRED_KEY};
use crate::{Container, Credentials, Field, KeyedEntry, Services, Value};

impl Section for Container<'static> {
    type Ast<'a> = Container<'a>;
}

/// `image`, a key of every container.
const IMAGE: KeySpec<Container<'static>> = KeySpec {
    name: "image",
    parse: |p, c, e| c.image = p.parse_string(e.key.string, e.value, false),
};

/// `credentials`, a key of every container.
const CREDENTIALS: KeySpec<Container<'static>> = KeySpec {
    name: "credentials",
    parse: |p, c, e| c.credentials = p.parse_credentials(e),
};

/// `env`, a key of every container.
const ENV: KeySpec<Container<'static>> = KeySpec {
    name: "env",
    parse: |p, c, e| c.env = p.parse_env(e.key, e.value),
};

/// `ports`, a key of every container.
const PORTS: KeySpec<Container<'static>> = KeySpec {
    name: "ports",
    parse: |p, c, e| {
        c.ports = p.parse_string_sequence(e.key.string, "ports", e.value, true, false);
    },
};

/// `volumes`, a key of every container. The reference stores it in
/// `Ports` (`docs/differences.md#container-volumes`).
const VOLUMES: KeySpec<Container<'static>> = KeySpec {
    name: "volumes",
    parse: |p, c, e| {
        c.volumes = p.parse_string_sequence(e.key.string, "volumes", e.value, true, false);
    },
};

/// `options`, a key of every container.
const OPTIONS: KeySpec<Container<'static>> = KeySpec {
    name: "options",
    parse: |p, c, e| c.options = p.parse_string(e.key.string, e.value, true),
};

/// The keys of a job's `container`.
static CONTAINER_KEYS: [KeySpec<Container<'static>>; 6] =
    [IMAGE, CREDENTIALS, ENV, PORTS, VOLUMES, OPTIONS];

/// The keys of a service: the keys of a job's `container`, `command`, and
/// `entrypoint`.
static SERVICE_KEYS: [KeySpec<Container<'static>>; 8] = [
    IMAGE,
    CREDENTIALS,
    ENV,
    PORTS,
    VOLUMES,
    OPTIONS,
    KeySpec {
        name: "command",
        parse: |p, c, e| c.command = p.parse_string(e.key.string, e.value, true),
    },
    KeySpec {
        name: "entrypoint",
        parse: |p, c, e| c.entrypoint = p.parse_string(e.key.string, e.value, true),
    },
];

impl Section for Credentials<'static> {
    type Ast<'a> = Credentials<'a>;
}

static CREDENTIALS_KEYS: [KeySpec<Credentials<'static>>; 2] = [
    KeySpec {
        name: "username",
        parse: |p, c, e| c.username = p.parse_string(e.key.string, e.value, false),
    },
    KeySpec {
        name: "password",
        parse: |p, c, e| c.password = p.parse_string(e.key.string, e.value, false),
    },
];

impl<'a> Parser<'a, '_> {
    /// Parses `container` of a job, the reference's `parseContainer` with
    /// the section `container`.
    ///
    /// A scalar is the image, with no key. A sequence or an alias gives an
    /// invalid field.
    pub(super) fn parse_job_container(&mut self, entry: &MapEntry<'a>) -> Field<'a, Container<'a>> {
        let node = entry.value;
        let container = self.parse_container(false, entry.key, node);
        match node.kind {
            NodeKind::Scalar(_) => Field::Valid {
                key: entry.key.string,
                span: node.span,
                value: container,
            },
            NodeKind::Mapping(_) | NodeKind::Sequence(_) | NodeKind::Alias(_) => {
                mapping_field(entry.key, node, container)
            }
        }
    }

    /// Parses `services` of a job, the reference's `parseServices`.
    ///
    /// A `!!str` scalar that is a single `${{ }}` expression is the
    /// services as an expression. Anything else is a case-insensitive
    /// mapping of service IDs to containers, which must have entries. A
    /// service whose ID is not a scalar is parsed, so its errors are
    /// reported, and then left out, because a [`Keyed`](crate::Keyed)
    /// collection needs a string key.
    pub(super) fn parse_services(&mut self, entry: &MapEntry<'a>) -> Value<'a, Services<'a>> {
        let key = entry.key.string;
        let node = entry.value;
        if let Some(expr) = Self::may_parse_expression(node) {
            return Value::Expression { key, expr };
        }
        let entries = self.parse_mapping(Label::Section("services"), node, false, false);
        let mut services = Vec::with_capacity(entries.len());
        for service in entries {
            let container = self.parse_container(true, service.key, service.value);
            if let Some(id) = service.key.string {
                services.push(KeyedEntry::new(id, service.value.span, container));
            }
        }
        match node.kind {
            NodeKind::Mapping(_) => Value::Literal {
                key,
                span: node.span,
                value: Services::new(services),
            },
            NodeKind::Scalar(_) | NodeKind::Sequence(_) | NodeKind::Alias(_) => Value::Invalid {
                key,
                span: node.span,
            },
        }
    }

    /// Parses `node` as a container, the reference's `parseContainer`. `key`
    /// is the key of `container` or the service ID.
    ///
    /// A scalar is the image. Otherwise `node` is a mapping, in whose
    /// messages the section is `services` for a `service` and `container`
    /// for a job's container. Without an `image` key it reports `"image" is
    /// missing in %q section` at `key`, also when `node` is not a mapping.
    /// An `image` with an invalid value counts as written.
    fn parse_container(&mut self, service: bool, key: Key<'a>, node: &'a Node) -> Container<'a> {
        if let NodeKind::Scalar(_) = node.kind {
            return Container {
                image: self.parse_string(None, node, false),
                ..Container::default()
            };
        }
        let (section, keys): (_, &[KeySpec<Container<'static>>]) = if service {
            ("services", &SERVICE_KEYS)
        } else {
            ("container", &CONTAINER_KEYS)
        };
        let mut container = Container::default();
        let entries = self.parse_mapping(Label::Section(section), node, false, true);
        self.parse_keys(entries, section, keys, &mut container);
        if !container.image.is_present() {
            self.error(
                &MISSING_REQUIRED_KEY,
                key.span,
                format!("\"image\" is missing in {} section", quote(section)),
            );
        }
        container
    }

    /// Parses `credentials` of a container, the reference's
    /// `parseCredentials`.
    ///
    /// A `!!str` scalar that is a single `${{ }}` expression is the
    /// credentials as an expression. Anything else is a mapping with
    /// `username` and `password`. Without either key it reports `both
    /// "username" and "password" must be specified in "credentials"
    /// section` at the section's key, also when the value is not a mapping,
    /// and the credentials are invalid. The reference drops them. A key with
    /// an invalid value counts as written.
    fn parse_credentials(&mut self, entry: &MapEntry<'a>) -> Value<'a, Credentials<'a>> {
        let key = entry.key.string;
        let node = entry.value;
        if let Some(expr) = Self::may_parse_expression(node) {
            return Value::Expression { key, expr };
        }
        let mut credentials = Credentials::default();
        let entries = self.parse_mapping(Label::Section("credentials"), node, false, true);
        self.parse_keys(entries, "credentials", &CREDENTIALS_KEYS, &mut credentials);
        if !credentials.username.is_present() || !credentials.password.is_present() {
            self.error(
                &INCOMPLETE_CREDENTIALS,
                entry.key.span,
                "both \"username\" and \"password\" must be specified in \"credentials\" section"
                    .to_owned(),
            );
            return Value::Invalid {
                key,
                span: node.span,
            };
        }
        // A value that is not a mapping has no keys, so it is reported as
        // incomplete above.
        Value::Literal {
            key,
            span: node.span,
            value: credentials,
        }
    }
}

#[cfg(test)]
mod tests;
