//! First-time placement of a flow diagram, standing in for ELK's layered algorithm:
//! break cycles, assign layers left to right, reduce crossings, then align with neighbours.
//! Edges that skip layers pass through dummy positions, and each group owns a horizontal band,
//! so lines do not cut through nodes and a frame never encloses a stranger.

use crate::board_layout::{node_w, Diagram, Rect, Route, NODE_H, P};
use crate::graph::Graph;
use std::collections::HashMap;

const NODE_GAP: f64 = 40.0;
const LAYER_GAP: f64 = 80.0;
const GROUP_TOP: f64 = 44.0;
const GROUP_PAD: f64 = 24.0;
const DUMMY_H: f64 = 12.0;
const DUMMY_GAP: f64 = 24.0;
const PORT_GAP: f64 = 12.0;

#[derive(Clone)]
struct Item {
    id: String,
    /// False for an edge's dummy position.
    real: bool,
    /// The group's band, if this is a member.
    band: Option<usize>,
    /// Bands take odd zones, and the free space around them even ones.
    zone: usize,
    h: f64,
    w: f64,
    y: f64,
}

/// One input edge laid left to right; `reversed` edges run against the flow and are flipped back.
struct Oriented {
    edge: usize,
    a: usize,
    b: usize,
    reversed: bool,
    chain: Vec<String>,
}

fn sweep_order(count: usize, forward: bool) -> Vec<usize> {
    if forward {
        (1..count).collect()
    } else {
        (0..count.saturating_sub(1)).rev().collect()
    }
}

pub fn first_diagram(g: &Graph, ids: &[String]) -> Diagram {
    let nodes: Vec<usize> = g
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| ids.contains(&n.id))
        .map(|(i, _)| i)
        .collect();
    let pos: HashMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .map(|(k, &i)| (g.nodes[i].id.as_str(), k))
        .collect();
    let edges: Vec<(usize, usize, usize)> = g
        .edges
        .iter()
        .enumerate()
        .filter_map(|(i, e)| Some((i, *pos.get(e.from.as_str())?, *pos.get(e.to.as_str())?)))
        .filter(|(_, a, b)| a != b)
        .collect();

    // Back edges found by DFS in model order are flipped for layering, like ELK's cycle breaking.
    let n = nodes.len();
    let mut state = vec![0_u8; n];
    let mut back = vec![false; edges.len()];
    fn visit(v: usize, edges: &[(usize, usize, usize)], state: &mut [u8], back: &mut [bool]) {
        state[v] = 1;
        for (k, e) in edges.iter().enumerate() {
            if e.1 == v {
                match state[e.2] {
                    0 => visit(e.2, edges, state, back),
                    1 => back[k] = true,
                    _ => {}
                }
            }
        }
        state[v] = 2;
    }
    for v in 0..n {
        if state[v] == 0 {
            visit(v, &edges, &mut state, &mut back);
        }
    }
    let mut oriented: Vec<Oriented> = edges
        .iter()
        .zip(&back)
        .map(|(&(edge, a, b), &reversed)| {
            let (a, b) = if reversed { (b, a) } else { (a, b) };
            Oriented {
                edge,
                a,
                b,
                reversed,
                chain: vec![],
            }
        })
        .collect();

    let mut layer = vec![0_usize; n];
    for _ in 0..=n {
        let mut moved = false;
        for o in &oriented {
            if layer[o.b] <= layer[o.a] {
                layer[o.b] = layer[o.a] + 1;
                moved = true;
            }
        }
        if !moved {
            break;
        }
    }
    // A node with no incoming edge sits next to what it feeds rather than far to the left.
    for v in 0..n {
        if !oriented.iter().any(|o| o.b == v) {
            if let Some(next) = oriented
                .iter()
                .filter(|o| o.a == v)
                .map(|o| layer[o.b])
                .min()
            {
                layer[v] = next.saturating_sub(1);
            }
        }
    }

    let bands: Vec<&str> = g
        .groups
        .iter()
        .map(|x| x.id.as_str())
        .filter(|id| {
            nodes
                .iter()
                .any(|&i| g.nodes[i].group.as_deref() == Some(*id))
        })
        .collect();
    let count = layer.iter().copied().max().map_or(0, |m| m + 1);
    let mut layers: Vec<Vec<Item>> = vec![vec![]; count];
    for (k, &i) in nodes.iter().enumerate() {
        let node = &g.nodes[i];
        let band = node
            .group
            .as_deref()
            .and_then(|x| bands.iter().position(|b| *b == x));
        layers[layer[k]].push(Item {
            id: node.id.clone(),
            real: true,
            band,
            zone: band.map_or(0, |b| b * 2 + 1),
            h: NODE_H,
            w: node_w(node),
            y: 0.0,
        });
    }
    let mut joints: Vec<(String, String)> = vec![];
    for (k, o) in oriented.iter_mut().enumerate() {
        let mut previous = g.nodes[nodes[o.a]].id.clone();
        for l in layer[o.a] + 1..layer[o.b] {
            let id = format!("#{k}:{l}");
            layers[l].push(Item {
                id: id.clone(),
                real: false,
                band: None,
                zone: 0,
                h: DUMMY_H,
                w: 0.0,
                y: 0.0,
            });
            joints.push((previous, id.clone()));
            previous = id.clone();
            o.chain.push(id);
        }
        joints.push((previous, g.nodes[nodes[o.b]].id.clone()));
    }
    let neighbours = |id: &str, left: bool| -> Vec<String> {
        joints
            .iter()
            .filter_map(|(a, b)| {
                if left && b == id {
                    Some(a.clone())
                } else if !left && a == id {
                    Some(b.clone())
                } else {
                    None
                }
            })
            .collect()
    };

    // Barycenter sweeps. Sorting is stable, so ties keep the order things were said in.
    for sweep in 0..8 {
        let left = sweep % 2 == 0;
        for l in sweep_order(count, left) {
            let other = if left { l - 1 } else { l + 1 };
            let rank: HashMap<String, f64> = layers[other]
                .iter()
                .enumerate()
                .map(|(i, it)| (it.id.clone(), i as f64))
                .collect();
            let mut keys: Vec<(f64, Item)> = layers[l]
                .iter()
                .enumerate()
                .map(|(i, it)| {
                    let seen: Vec<f64> = neighbours(&it.id, left)
                        .iter()
                        .filter_map(|o| rank.get(o).copied())
                        .collect();
                    (
                        if seen.is_empty() {
                            i as f64
                        } else {
                            seen.iter().sum::<f64>() / seen.len() as f64
                        },
                        it.clone(),
                    )
                })
                .collect();
            keys.sort_by(|a, b| a.0.total_cmp(&b.0));
            layers[l] = keys.into_iter().map(|(_, it)| it).collect();
        }
    }

    // Everything outside a group picks the free zone closest to the neighbours it connects to.
    if !bands.is_empty() {
        for items in &mut layers {
            let mut seen = 0;
            for it in items.iter_mut() {
                match it.band {
                    Some(b) => seen = seen.max((b + 1) * 2),
                    None => it.zone = seen,
                }
            }
        }
        for sweep in 0..6 {
            let left = sweep % 2 == 0;
            for l in sweep_order(count, left) {
                let zones: HashMap<String, usize> = layers
                    .iter()
                    .flatten()
                    .map(|it| (it.id.clone(), it.zone))
                    .collect();
                for it in layers[l].iter_mut().filter(|it| it.band.is_none()) {
                    let seen: Vec<usize> = neighbours(&it.id, left)
                        .iter()
                        .filter_map(|o| zones.get(o).copied())
                        .collect();
                    if !seen.is_empty() {
                        let mean = seen.iter().sum::<usize>() as f64 / seen.len() as f64;
                        // Round to the nearest free zone, which are the even ones.
                        it.zone = ((mean / 2.0).round() as usize * 2).min(bands.len() * 2);
                    }
                }
            }
        }
        for items in &mut layers {
            items.sort_by_key(|it| it.zone);
        }
    }

    // Stack each zone by itself, align items with their neighbours, then place zones in sequence.
    let gap = |a: &Item, b: &Item| {
        if a.real && b.real {
            NODE_GAP
        } else {
            DUMMY_GAP
        }
    };
    let zones = bands.len() * 2 + 1;
    let mut offset = 0.0;
    for zone in 0..zones {
        for items in &mut layers {
            let mut y = 0.0;
            let mut previous: Option<usize> = None;
            for i in 0..items.len() {
                if items[i].zone != zone {
                    continue;
                }
                if let Some(p) = previous {
                    y += gap(&items[p], &items[i]);
                }
                items[i].y = y;
                y += items[i].h;
                previous = Some(i);
            }
        }
        for pass in 0..8 {
            let left = pass % 2 == 0;
            for l in sweep_order(count, left) {
                let centers: HashMap<String, f64> = layers
                    .iter()
                    .flatten()
                    .filter(|it| it.zone == zone)
                    .map(|it| (it.id.clone(), it.y + it.h / 2.0))
                    .collect();
                let items = &mut layers[l];
                let mut previous: Option<usize> = None;
                for i in 0..items.len() {
                    if items[i].zone != zone {
                        continue;
                    }
                    let seen: Vec<f64> = neighbours(&items[i].id, left)
                        .iter()
                        .filter_map(|o| centers.get(o).copied())
                        .collect();
                    let wanted = if seen.is_empty() {
                        items[i].y
                    } else {
                        seen.iter().sum::<f64>() / seen.len() as f64 - items[i].h / 2.0
                    };
                    let least = previous.map_or(f64::MIN, |p| {
                        items[p].y + items[p].h + gap(&items[p], &items[i])
                    });
                    items[i].y = wanted.max(least);
                    previous = Some(i);
                }
            }
        }
        let top = layers
            .iter()
            .flatten()
            .filter(|it| it.zone == zone)
            .map(|it| it.y)
            .fold(f64::MAX, f64::min);
        let bottom = layers
            .iter()
            .flatten()
            .filter(|it| it.zone == zone)
            .map(|it| it.y + it.h)
            .fold(f64::MIN, f64::max);
        let used = bottom >= top;
        let is_band = zone % 2 == 1;
        // Room for the frame around a band, and for the title along its top.
        if is_band && (zone > 1 || offset > 0.0 || used) {
            offset += GROUP_TOP;
        }
        if used {
            for it in layers.iter_mut().flatten().filter(|it| it.zone == zone) {
                it.y += offset - top;
            }
            offset += bottom - top;
            offset += if is_band {
                GROUP_PAD + NODE_GAP
            } else {
                NODE_GAP
            };
        }
    }

    // Nodes centre in their layer; a line crosses the rest of the layer's width horizontally,
    // so a narrow node's edge cannot slice through a wider neighbour.
    let mut x = if layers
        .first()
        .is_some_and(|l| l.iter().any(|i| i.band.is_some()))
    {
        GROUP_PAD
    } else {
        0.0
    };
    let mut placed: Vec<Rect> = vec![];
    let mut dummy: HashMap<String, (f64, f64, f64)> = HashMap::new();
    let mut span: HashMap<String, (f64, f64)> = HashMap::new();
    for items in &layers {
        let width = items.iter().map(|i| i.w).fold(0.0, f64::max);
        for it in items {
            span.insert(it.id.clone(), (x, x + width));
            if it.real {
                placed.push(Rect {
                    id: it.id.clone(),
                    x: x + (width - it.w) / 2.0,
                    y: it.y,
                    w: it.w,
                    h: it.h,
                });
            } else {
                dummy.insert(it.id.clone(), (x, x + width, it.y + it.h / 2.0));
            }
        }
        x += width + LAYER_GAP;
    }
    placed.sort_by_key(|b| pos[b.id.as_str()]);
    let find = |id: &str| placed.iter().find(|b| b.id == id).expect("placed");

    // Several edges on one side of a node spread out, ordered by where they head.
    let next_y = |o: &Oriented, from_a: bool| -> f64 {
        let waypoint = if from_a {
            o.chain.first()
        } else {
            o.chain.last()
        };
        waypoint.map_or_else(
            || {
                let far = find(&g.nodes[nodes[if from_a { o.b } else { o.a }]].id);
                far.y + far.h / 2.0
            },
            |id| dummy[id].2,
        )
    };
    let port = |node: usize, o_index: usize, from_a: bool| -> f64 {
        let mut same: Vec<usize> = oriented
            .iter()
            .enumerate()
            .filter(|(_, o)| if from_a { o.a == node } else { o.b == node })
            .map(|(i, _)| i)
            .collect();
        same.sort_by(|&p, &q| {
            next_y(&oriented[p], from_a).total_cmp(&next_y(&oriented[q], from_a))
        });
        let rank = same.iter().position(|&i| i == o_index).expect("listed") as f64;
        let b = find(&g.nodes[nodes[node]].id);
        b.y + b.h / 2.0 + (rank - (same.len() as f64 - 1.0) / 2.0) * PORT_GAP
    };

    let mut routes = vec![];
    for (k, o) in oriented.iter().enumerate() {
        let e = &g.edges[o.edge];
        let (a, b) = (find(&g.nodes[nodes[o.a]].id), find(&g.nodes[nodes[o.b]].id));
        let (ya, yb) = (port(o.a, k, true), port(o.b, k, false));
        let mut points: Vec<P> = vec![[a.x + a.w, ya]];
        let right = span[&a.id].1;
        if right > a.x + a.w + 0.5 {
            points.push([right, ya]);
        }
        for id in &o.chain {
            let (left, right, y) = dummy[id];
            points.push([left, y]);
            if right > left {
                points.push([right, y]);
            }
        }
        let left = span[&b.id].0;
        if left < b.x - 0.5 {
            points.push([left, yb]);
        }
        points.push([b.x, yb]);
        if o.reversed {
            points.reverse();
        }
        routes.push(Route {
            from: e.from.clone(),
            to: e.to.clone(),
            points,
            label_box: None,
        });
    }

    let mut groups = vec![];
    for grp in &g.groups {
        let members: Vec<&Rect> = placed
            .iter()
            .filter(|b| {
                g.nodes
                    .iter()
                    .any(|n| n.id == b.id && n.group.as_ref() == Some(&grp.id))
            })
            .collect();
        if members.is_empty() {
            continue;
        }
        let gx = members.iter().map(|b| b.x).fold(f64::MAX, f64::min) - GROUP_PAD;
        let gy = members.iter().map(|b| b.y).fold(f64::MAX, f64::min) - GROUP_TOP;
        let gx1 = members.iter().map(|b| b.x + b.w).fold(f64::MIN, f64::max) + GROUP_PAD;
        let gy1 = members.iter().map(|b| b.y + b.h).fold(f64::MIN, f64::max) + GROUP_PAD;
        groups.push((
            Rect {
                id: grp.id.clone(),
                x: gx,
                y: gy,
                w: gx1 - gx,
                h: gy1 - gy,
            },
            grp.label.clone(),
        ));
    }

    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for b in placed.iter().chain(groups.iter().map(|(b, _)| b)) {
        x0 = x0.min(b.x);
        y0 = y0.min(b.y);
        x1 = x1.max(b.x + b.w);
        y1 = y1.max(b.y + b.h);
    }
    for p in routes.iter().flat_map(|r| &r.points) {
        x0 = x0.min(p[0]);
        y0 = y0.min(p[1]);
        x1 = x1.max(p[0]);
        y1 = y1.max(p[1]);
    }
    if placed.is_empty() {
        (x0, y0, x1, y1) = (0.0, 0.0, 0.0, 0.0);
    }
    // The caller treats a first layout as starting at the origin.
    for b in placed.iter_mut().chain(groups.iter_mut().map(|(b, _)| b)) {
        b.x -= x0;
        b.y -= y0;
    }
    for r in &mut routes {
        for p in &mut r.points {
            p[0] -= x0;
            p[1] -= y0;
        }
    }
    Diagram {
        x0: 0.0,
        y0: 0.0,
        w: x1 - x0,
        h: y1 - y0,
        nodes: placed,
        groups,
        edges: routes,
    }
}
