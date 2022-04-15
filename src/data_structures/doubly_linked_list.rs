use alloc::boxed::Box;
use core::mem;

#[repr(transparent)]
pub struct DoublyLinkedList<T>(DoublyLinkedListNode<T>);

impl<T> DoublyLinkedList<T> {

    pub fn new(val: T) -> Self {
        DoublyLinkedListNode::new(val).to_list()
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

    pub fn next(&self) -> &Option<DoublyLinkedListNode<T>> {
        self.0.next.as_ref()
    }

    pub fn next_mut(&mut self) -> &mut Option<DoublyLinkedListNode<T>> {
        self.0.next.as_mut()
    }

}

pub struct DoublyLinkedListNode<T> {
    pub val: T,
    pub last: Box<Option<DoublyLinkedListNode<T>>>, // FIXME: try changing this to: Option<Box<DoublyLinkedListNode<T>>>
    pub next: Box<Option<DoublyLinkedListNode<T>>>, // FIXME: try changing this to: Option<Box<DoublyLinkedListNode<T>>>
}

impl<T> DoublyLinkedListNode<T> {

    pub fn new(val: T) -> Self {
        Self {
            val,
            last: Box::new(None),
            next: Box::new(None),
        }
    }

    #[inline(always)]
    pub fn to_list(self) -> DoublyLinkedList<T> {
        DoublyLinkedList(self)
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

    pub fn next(&self) -> &Option<DoublyLinkedListNode<T>> {
        self.next.as_ref()
    }

    pub fn next_mut(&mut self) -> &mut Option<DoublyLinkedListNode<T>> {
        self.next.as_mut()
    }

    pub fn replace_next(&mut self, val: T) -> Option<DoublyLinkedListNode<T>> {
        self.next.replace(DoublyLinkedListNode::new(val))
    }

    pub fn replace_next_raw(&mut self, node: DoublyLinkedListNode<T>) -> Option<DoublyLinkedListNode<T>> {
        self.next.replace(node)
    }

    pub fn last(&self) -> &Option<DoublyLinkedListNode<T>> {
        self.last.as_ref()
    }

    pub fn last_mut(&mut self) -> &mut Option<DoublyLinkedListNode<T>> {
        self.last.as_mut()
    }

    pub fn replace_last(&mut self, val: T) -> Option<DoublyLinkedListNode<T>> {
        self.last.replace(DoublyLinkedListNode::new(val))
    }

    pub fn replace_last_raw(&mut self, node: DoublyLinkedListNode<T>) -> Option<DoublyLinkedListNode<T>> {
        self.last.replace(node)
    }

}