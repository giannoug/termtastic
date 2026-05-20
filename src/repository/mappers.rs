use super::entities;
use crate::types::{Node, NodeUser};

impl From<Node> for entities::v1::Node {
    fn from(value: Node) -> Self {
        Self::from(&value)
    }
}

impl From<&Node> for entities::v1::Node {
    fn from(value: &Node) -> Self {
        let user = value.user.as_ref().expect("should be Some");

        Self {
            key: value.key,
            hops: value.hops,
            last_heard: value.last_heard,
            snr: value.snr,
            rssi: value.rssi,
            is_favorite: value.is_favorite,
            is_ignored: value.is_ignored,
            is_muted: value.is_muted,
            user_id: user.id.clone(),
            user_short_name: user.short_name.clone(),
            user_long_name: user.long_name.clone(),
            user_role: user.role,
            user_hw_model: user.hw_model,
            user_public_key: user.public_key.clone(),
            user_is_licensed: user.is_licensed,
            user_is_unmessagable: user.is_unmessagable,
        }
    }
}

impl From<entities::v1::Node> for Node {
    fn from(value: entities::v1::Node) -> Self {
        let mut node = Self {
            key: value.key,
            user: Some(NodeUser {
                id: value.user_id,
                short_name: value.user_short_name,
                long_name: value.user_long_name,
                role: value.user_role,
                hw_model: value.user_hw_model,
                public_key: value.user_public_key,
                is_licensed: value.user_is_licensed,
                is_unmessagable: value.user_is_unmessagable,
            }),
            hops: value.hops,
            last_heard: value.last_heard,
            snr: value.snr,
            rssi: value.rssi,
            is_favorite: value.is_favorite,
            is_ignored: value.is_ignored,
            is_muted: value.is_muted,
            fulltext: String::default(),
        };

        node.update_fulltext();

        node
    }
}
