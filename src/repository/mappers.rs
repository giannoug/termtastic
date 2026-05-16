use super::entities;
use crate::types::{Node, NodeUser};

impl From<Node> for entities::v1::Node {
    fn from(value: Node) -> Self {
        Self::from(&value)
    }
}

impl From<&Node> for entities::v1::Node {
    fn from(value: &Node) -> Self {
        Self {
            id: value.id.clone(),
            key: value.key,
            user: value.user.as_ref().map(Into::into),
            hops: value.hops,
            last_heard: value.last_heard,
            snr: value.snr,
            rssi: value.rssi,
            public_key: value.public_key.clone(),
            is_favorite: value.is_favorite,
            is_ignored: value.is_ignored,
            is_muted: value.is_muted,
        }
    }
}

impl From<entities::v1::Node> for Node {
    fn from(value: entities::v1::Node) -> Self {
        let mut node = Self {
            id: value.id,
            key: value.key,
            user: value.user.map(Into::into),
            hops: value.hops,
            last_heard: value.last_heard,
            snr: value.snr,
            rssi: value.rssi,
            public_key: value.public_key,
            is_favorite: value.is_favorite,
            is_ignored: value.is_ignored,
            is_muted: value.is_muted,
            fulltext: "".to_string(),
        };

        node.update_fulltext();

        node
    }
}

impl From<NodeUser> for entities::v1::NodeUser {
    fn from(value: NodeUser) -> Self {
        Self::from(&value)
    }
}

impl From<&NodeUser> for entities::v1::NodeUser {
    fn from(value: &NodeUser) -> Self {
        Self {
            short_name: value.short_name.clone(),
            long_name: value.long_name.clone(),
            role: value.role,
            hw_model: value.hw_model,
            is_licensed: value.is_licensed,
            is_unmessagable: value.is_unmessagable,
        }
    }
}

impl From<entities::v1::NodeUser> for NodeUser {
    fn from(value: entities::v1::NodeUser) -> Self {
        Self {
            short_name: value.short_name,
            long_name: value.long_name,
            role: value.role,
            hw_model: value.hw_model,
            is_licensed: value.is_licensed,
            is_unmessagable: value.is_unmessagable,
        }
    }
}
