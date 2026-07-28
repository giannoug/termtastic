use crate::state::State;
use crate::types::TracerouteItem;

const SNR_UNKNOWN: i32 = -128;

fn snr_from_q4(raw: i32) -> Option<f32> {
    if raw == SNR_UNKNOWN {
        None
    } else {
        Some(raw as f32 / 4.0)
    }
}

fn node_label(state: &State, num: u32) -> String {
    match state.nodes.get(&num) {
        Some(node) => format!("{} ({})", node.long_name(), node.id()),
        None => format!("!{:x}", num),
    }
}

pub fn update_nodeinfo_traceroute(state: &mut State) -> bool {
    let Some(node_key) = state.nodeinfo else {
        return false;
    };

    let Some(traceroute) = state.nodes_traceroute.get(&node_key).cloned() else {
        if state.nodeinfo_traceroute.is_empty() {
            return false;
        }

        state.nodeinfo_traceroute.clear();
        return true;
    };

    let my_node_key = state.my_node_key;
    let mut items: Vec<TracerouteItem> = Vec::new();

    // route towards the destination: [me] -> [intermediate hops] -> [destination]
    let mut forward: Vec<u32> = Vec::new();
    if let Some(my) = my_node_key {
        forward.push(my);
    }
    forward.extend(traceroute.route.iter().copied());
    forward.push(node_key);

    items.push(TracerouteItem::group(
        "Route there",
        traceroute.datetime,
        serde_json::to_string_pretty(&serde_json::json!({
            "route": traceroute.route,
            "snr_towards": traceroute.snr_towards,
        }))
        .unwrap_or_else(|_| "serialize failed".to_owned()),
    ));

    for (i, num) in forward.iter().enumerate() {
        let snr = if i == 0 {
            None
        } else {
            traceroute.snr_towards.get(i - 1).copied().and_then(snr_from_q4)
        };

        items.push(TracerouteItem::hop(node_label(state, *num), snr));
    }

    // route back to us: [destination] -> [intermediate hops] -> [me]
    if !traceroute.route_back.is_empty() || !traceroute.snr_back.is_empty() {
        let mut back: Vec<u32> = Vec::new();
        back.push(node_key);
        back.extend(traceroute.route_back.iter().copied());
        if let Some(my) = my_node_key {
            back.push(my);
        }

        items.push(TracerouteItem::group(
            "Route back",
            traceroute.datetime,
            serde_json::to_string_pretty(&serde_json::json!({
                "route_back": traceroute.route_back,
                "snr_back": traceroute.snr_back,
            }))
            .unwrap_or_else(|_| "serialize failed".to_owned()),
        ));

        for (i, num) in back.iter().enumerate() {
            let snr = if i == 0 {
                None
            } else {
                traceroute.snr_back.get(i - 1).copied().and_then(snr_from_q4)
            };

            items.push(TracerouteItem::hop(node_label(state, *num), snr));
        }
    }

    state.nodeinfo_traceroute = items;

    true
}
