/*
pub struct RBTree<T: PartialOrd> {
    root: Option<RBTreeNode<T>>,
}

impl<T: PartialOrd> RBTree<T> {
    
    pub fn new() -> Self {
        Self {
            root: None
        }
    }

    pub fn insert(&mut self, val: T) {
        if let Some(node) = &mut self.root {
            if node.insert(val) {
                // FIXME: Handle node recoloring
            }
        } else {
            self.root = Some(RBTreeNode {
                red: false,
                left: None,
                right: None,
                val,
            });
        }
    }
    
}

pub struct RBTreeNode<T: PartialOrd> {
    red: bool,
    left: Option<RBTreeNode<T>>,
    right: Option<RBTreeNode<T>>,
    val: T,
}

impl<T: PartialOrd> RBTreeNode<T> {

    /// Returns whether the node has to be recolored
    fn insert(&mut self, val: T) -> Option<T> {
        let mut node = if self.val < val {
            &mut self.right
        } else {
            &mut self.left
        };

        if let Some(node) = node {
            if let Some(val) = node.insert(val) {
                // FIXME: handle recoloring

            } else {
                None
            }
        } else {
            if red {
                // we need to recolor this node
                Some(val)
            } else {
                *node = Some(RBTreeNode {
                    red: true,
                    left: None,
                    right: None,
                    val,
                });
                None
            }
        }

        None
    }

}*/