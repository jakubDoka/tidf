use std::fmt::{Debug, Display, Write};

use crate::prelude::*;

pub trait QuadElement: PartialEq + Eq + Clone + Debug {}
impl<T: PartialEq + Eq + Clone + Debug> QuadElement for T {}

#[derive(Debug, Clone)]
pub struct QuadTree<T: QuadElement, G: QuadElement> {
    pub cap: usize,
    nodes: Vec<QuadNode<T, G>>,
}

impl<T: QuadElement, G: QuadElement> QuadTree<T, G> {
    pub fn new(rect: Rectangle, cap: usize) -> QuadTree<T, G> {
        QuadTree {
            cap,
            // first one is null
            nodes: vec![
                QuadNode::new(Rectangle::default(), 0),
                QuadNode::new(rect, 0),
            ],
        }
    }

    pub fn query(&self, area: Rectangle, group: G, include: bool, buffer: &mut Vec<T>) {
        self.query_low(QuadPointer(1), area, group, include, buffer)
    }

    fn query_low(
        &self,
        from: QuadPointer,
        area: Rectangle,
        group: G,
        include: bool,
        buffer: &mut Vec<T>,
    ) {
        let node = &self.nodes[from.index()];
        node.storage.collect(group.clone(), include, buffer);
        if node.children.is_null() {
            return;
        }
        for i in node.children.index()..node.children.index() + 4 {
            let child = &self.nodes[i];
            if child.total != 0 && child.bounds.check_collision_recs(&area) {
                self.query_low(QuadPointer::new(i), area, group.clone(), include, buffer);
            }
        }
    }

    pub fn insert(&mut self, bounds: Rectangle, data: T, group: G) -> QuadPointer {
        let best_id = self.find_fitting_node(QuadPointer(1), bounds, true);
        let best_node = &mut self.nodes[best_id.index()];
        best_node.storage.add(data, group);
        if best_node.children.is_null() && best_node.storage.count() > self.cap {
            self.split(best_id);
        }
        best_id
    }

    pub fn update(
        &mut self,
        bounds: Rectangle,
        pointer: QuadPointer,
        data: T,
        group: G,
    ) -> QuadPointer {
        let best_id = self.find_fitting_node(pointer, bounds, false);
        if best_id != pointer {
            self.nodes[pointer.index()]
                .storage
                .remove(data.clone(), group.clone());
            let best_node = &mut self.nodes[best_id.index()];
            best_node.storage.add(data, group);
            if best_node.children.is_null() && best_node.storage.count() > self.cap {
                self.split(best_id);
            }
        }
        best_id
    }

    pub fn remove(&mut self, mut pointer: QuadPointer, data: T, group: G) {
        self.nodes[pointer.index()].storage.remove(data, group);

        while !pointer.is_null() {
            self.nodes[pointer.index()].total -= 1;
            pointer = self.nodes[pointer.index()].parent;
        }
    }

    pub fn split(&mut self, id: QuadPointer) {
        let new_id = self.nodes.len();
        let node = &mut self.nodes[id.index()];
        node.children.0 = new_id as u32;
        let bounds = node.bounds;
        let center = bounds.center();
        self.nodes.extend([
            QuadNode::new(
                Rectangle::new(bounds.x, bounds.y, center.x, center.y),
                id.index(),
            ),
            QuadNode::new(
                Rectangle::new(center.x, bounds.y, bounds.right(), center.y),
                id.index(),
            ),
            QuadNode::new(
                Rectangle::new(center.x, center.y, bounds.right(), bounds.top()),
                id.index(),
            ),
            QuadNode::new(
                Rectangle::new(bounds.x, center.y, center.x, bounds.top()),
                id.index(),
            ),
        ]);
    }

    #[inline]
    fn find_fitting_node(
        &mut self,
        mut current: QuadPointer,
        rect: Rectangle,
        new: bool,
    ) -> QuadPointer {
        loop {
            let current_node = &mut self.nodes[current.index()];
            if current_node.bounds.fits_in(&rect) && current_node.total >= self.cap {
                break;
            }
            if current_node.parent.is_null() {
                if new {
                    current_node.total += 1;
                }
                return current;
            }
            current_node.total -= 1;
            current = current_node.parent;
        }

        if !new {
            self.nodes[current.index()].total -= 1;
        }

        loop {
            let current_node = &mut self.nodes[current.index()];
            current_node.total += 1;
            if current_node.children.is_null() || current_node.total < self.cap {
                return current;
            }

            let center = current_node.bounds.center();
            let (left, right) = (rect.right() < center.x, rect.x > center.x);
            let (top, bottom) = (rect.y > center.y, rect.top() < center.y);

            current.0 = current_node.children.0 as u32
                + if left {
                    if top {
                        3
                    } else if bottom {
                        0
                    } else {
                        break current;
                    }
                } else if right {
                    if top {
                        2
                    } else if bottom {
                        1
                    } else {
                        break current;
                    }
                } else {
                    break current;
                };
        }
    }

    fn log(
        &self,
        from: QuadPointer,
        level: usize,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        let node = &self.nodes[from.index()];
        std::iter::repeat(' ')
            .take(level)
            .for_each(|ch| f.write_char(ch).unwrap());
        write!(f, "{} {:?} {}\n", node.storage, node.bounds, node.total)?;
        if !node.children.is_null() && node.total != 0 {
            for i in node.children.0..node.children.0 + 4 {
                self.log(QuadPointer(i), level + 1, f)?;
            }
        }
        Ok(())
    }

    #[inline]
    pub fn total(&self) -> usize {
        self.nodes[1].total
    }

    pub fn resize(&mut self, area: Rectangle) {
        self.resize_low(1, area);
    }

    fn resize_low(&mut self, target: usize, area: Rectangle) {
        let node = &mut self.nodes[target];
        node.bounds = area;
        if node.children.is_null() {
            return;
        }
        let children = node.children.index();
        let center = area.center();
        self.resize_low(
            children + 0,
            Rectangle::new(area.x, area.y, center.x, center.y),
        );
        self.resize_low(
            children + 1,
            Rectangle::new(center.x, area.y, area.right(), center.y),
        );
        self.resize_low(
            children + 2,
            Rectangle::new(center.x, center.y, area.right(), area.top()),
        );
        self.resize_low(
            children + 3,
            Rectangle::new(area.x, center.y, center.x, area.top()),
        );
    }
}

impl<T: QuadElement, G: QuadElement> Display for QuadTree<T, G> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.log(QuadPointer(1), 0, f)
    }
}

#[derive(Debug, Clone)]
struct QuadNode<T: QuadElement, G: QuadElement> {
    bounds: Rectangle,
    storage: Tile<T, G>,
    children: QuadPointer,
    parent: QuadPointer,
    total: usize,
}

impl<T: QuadElement, G: QuadElement> QuadNode<T, G> {
    pub fn new(rect: Rectangle, parent: usize) -> QuadNode<T, G> {
        QuadNode {
            bounds: rect,
            storage: Tile::default(),
            children: QuadPointer(0),
            parent: QuadPointer::new(parent),
            total: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct QuadPointer(u32);

impl QuadPointer {
    fn new(id: usize) -> QuadPointer {
        QuadPointer(id as u32)
    }

    #[inline]
    pub fn is_null(&self) -> bool {
        self.0 == 0
    }

    pub fn index(&self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Debug)]
pub struct Tile<T: QuadElement, G: QuadElement> {
    items: Vec<Item<T, G>>,
    count: usize,
}

impl<T: QuadElement, G: QuadElement> Default for Tile<T, G> {
    fn default() -> Self {
        Tile {
            items: vec![],
            count: 0,
        }
    }
}

impl<T: QuadElement, G: QuadElement> Display for Tile<T, G> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        for item in &self.items {
            write!(f, "{} ", item)?;
        }

        write!(f, "|")?;

        Ok(())
    }
}

impl<T: QuadElement, G: QuadElement> Tile<T, G> {
    pub fn add(&mut self, t: T, g: G) {
        self.count += 1;
        let (i, size) = match self.find_group(g.clone()) {
            Some(val) => val,
            None => {
                self.items.push(Item::GroupHeader(g.clone(), 0));
                (self.items.len() - 1, 0)
            }
        };
        self.items[i] = Item::GroupHeader(g, size + 1);
        self.items.insert(i + 1, Item::Item(t));
    }

    pub fn remove(&mut self, t: T, g: G) {
        self.count -= 1;
        let (i, size) = self.find_group(g.clone()).expect(&format!(
            "removing from group that does not exist ({:?})",
            g
        ));
        for j in i + 1..i + size + 1 {
            if self.items[j] == Item::Item(t.clone()) {
                self.items.remove(j);
                if size == 1 {
                    self.items.remove(i);
                } else {
                    self.items[i] = Item::GroupHeader(g, size - 1);
                }
                return;
            }
        }

        panic!("removing item that does not exist (t: {:?} g: {:?})", t, g);
    }

    pub fn collect(&self, g: G, include: bool, buffer: &mut Vec<T>) {
        let mut i = 0;
        while i < self.items.len() {
            match self.items[i].clone() {
                Item::GroupHeader(ag, size) => {
                    if (g == ag && include) || (g != ag && !include) {
                        buffer.extend(self.items[i + 1..i + size + 1].iter().map(|x| match x {
                            Item::Item(t) => t.clone(),
                            _ => unreachable!(),
                        }));
                    }
                    i += size + 1;
                }
                _ => unreachable!(),
            }
        }
    }

    pub fn find_group(&self, g: G) -> Option<(usize, usize)> {
        let mut i = 0;
        while i < self.items.len() {
            match self.items[i].clone() {
                Item::GroupHeader(ag, size) => {
                    if g == ag {
                        return Some((i, size));
                    } else {
                        i += size + 1;
                    }
                }
                _ => unreachable!(),
            }
        }

        None
    }

    pub fn count(&self) -> usize {
        self.count
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Item<T: QuadElement, G: QuadElement> {
    GroupHeader(G, usize),
    Item(T),
}

impl<T: QuadElement, G: QuadElement> Display for Item<T, G> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Item::GroupHeader(g, size) => write!(f, "|{:?} {:?}|", g, size),
            Item::Item(t) => write!(f, "{:?}", t),
        }
    }
}

#[cfg(test)]
mod test {
    use std::time::Instant;

    use crate::{
        prelude::*,
        quad_tree::{QuadPointer, QuadTree},
    };

    //#[test]
    fn fuzzing() {
        let bounds = Rectangle::new(0.0, 0.0, 10000.0, 10000.0);
        let mut tree = QuadTree::<usize, usize>::new(bounds, 5);

        struct Obj {
            addr: QuadPointer,
            pos: Vector2,
        }

        let amount = 10_000;

        let mut objects = (0..amount)
            .into_iter()
            .map(|i| {
                let pos = Vector2::new(
                    rand::random::<f32>() * bounds.width,
                    rand::random::<f32>() * bounds.height,
                );
                Obj {
                    addr: tree.insert(Rectangle::square(pos, 1.0), i, 1),
                    pos,
                }
            })
            .collect::<Vec<_>>();

        println!("{}", tree.nodes[1].total);
        let instant = Instant::now();
        for _i in 0..1000 {
            for i in 0..amount {
                let mut obj = &mut objects[i];
                let step = Vector2::rad(rand::random::<f32>() * 2.0 * std::f32::consts::PI, 10.0);
                obj.pos += step;
                obj.addr = tree.update(Rectangle::square(obj.pos, 1.0), obj.addr, i, 1);
                if obj.pos.x < 0.0 {
                    obj.pos.x += bounds.width;
                } else if obj.pos.x > bounds.width {
                    obj.pos.x -= bounds.width;
                } else if obj.pos.y < 0.0 {
                    obj.pos.y += bounds.height;
                } else if obj.pos.y > bounds.height {
                    obj.pos.y -= bounds.height;
                }
            }
        }
        println!("{}", Instant::now().duration_since(instant).as_secs_f32());
        println!("{}", tree.nodes[1].total);

        for i in 0..amount {
            let obj = &objects[i];
            tree.remove(obj.addr, i, 1);
        }

        println!("{}", tree);
    }

    #[test]
    fn quad_tree() {
        let mut qt = QuadTree::<usize, usize>::new(Rectangle::new(0.0, 0.0, 100.0, 100.0), 1);

        let mut nodes = vec![];
        for i in 0..10 {
            nodes.push(qt.insert(Rectangle::new(20.0, 20.0, 20.0, 20.0), i, 1));
        }

        println!("{}", qt);

        for (i, id) in nodes.iter_mut().enumerate() {
            *id = qt.update(Rectangle::new(80.0, 80.0, 80.0, 80.0), *id, i, 1);
        }

        println!("{}", qt);

        for (i, id) in nodes.iter().enumerate() {
            qt.remove(*id, i, 1);
        }

        println!("{}", qt);
    }
}
