use alloc::boxed::Box;
use core::mem;

#[repr(transparent)]
pub struct LinkedList<T>(LinkedListNode<T>);

impl<T> LinkedList<T> {

    pub fn new(val: T) -> Self {
        LinkedListNode::new(val).to_list()
    }

    #[inline]
    pub fn value(&self) -> &T {
        &self.0.val
    }

    #[inline]
    pub fn value_mut(&mut self) -> &mut T {
        &mut self.0.val
    }

    pub fn set_value(&mut self, val: T) -> T {
        mem::replace(&mut self.0.val, val)
    }

    pub fn next(&self) -> &Option<LinkedListNode<T>> {
        self.0.next.as_ref()
    }

    pub fn next_mut(&mut self) -> &mut Option<LinkedListNode<T>> {
        self.0.next.as_mut()
    }

}

pub struct LinkedListNode<T> {
    val: T,
    next: Box<Option<LinkedListNode<T>>>, // FIXME: try changing this to: Option<Box<DoublyLinkedListNode<T>>>
}

impl<T> LinkedListNode<T> {

    pub fn new(val: T) -> Self {
        Self {
            val,
            next: Box::new(None),
        }
    }

    #[inline(always)]
    pub fn to_list(self) -> LinkedList<T> {
        LinkedList(self)
    }

    #[inline]
    pub fn value(&self) -> &T {
        &self.val
    }

    #[inline]
    pub fn value_mut(&mut self) -> &mut T {
        &mut self.val
    }

    pub fn set_value(&mut self, val: T) -> T {
        mem::replace(&mut self.val, val)
    }

    pub fn next(&self) -> &Option<LinkedListNode<T>> {
        self.next.as_ref()
    }

    pub fn next_mut(&mut self) -> &mut Option<LinkedListNode<T>> {
        self.next.as_mut()
    }

    pub fn replace_next(&mut self, val: T) -> Option<LinkedListNode<T>> {
        self.next.replace(LinkedListNode::new(val))
    }

    pub fn replace_next_raw(&mut self, node: LinkedListNode<T>) -> Option<LinkedListNode<T>> {
        self.next.replace(node)
    }

}