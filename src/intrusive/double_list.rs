use core::marker::PhantomData;
use core::cell::UnsafeCell;

pub struct Link<T> {
    owner: *mut T,
    prev: *mut Link<T>,
    next: *mut Link<T>,
}

impl<T> Link<T> {
    pub const fn new() -> Self {
        Self {
            owner: core::ptr::null_mut(),
            prev: core::ptr::null_mut(),
            next: core::ptr::null_mut(),
        }
    }

    pub fn owner(&self) -> Option<&T> {
        unsafe { self.owner.as_ref() }
    }

    pub fn owner_mut(&mut self) -> Option<&mut T> {
        unsafe { self.owner.as_mut() }
    }

    pub fn unlink(&mut self) {
        unsafe {
            if !self.prev.is_null() {
                (&mut *self.prev).next = self.next;
            }

            if !self.next.is_null() {
                (&mut *self.next).prev = self.prev;
            }

            self.prev = core::ptr::null_mut();
            self.next = core::ptr::null_mut();
        }
    }
}

impl<T> Drop for Link<T> {
    fn drop(&mut self) {
        self.unlink();
    }
}

pub struct Node<T> {
    pub data: T,
    link: Link<T>,
}

impl<T> Node<T> {
    pub fn new(value: T) -> Self {
        Self {
            data: value,
            link: Link::new()
        }
    }

    pub fn unlink(&mut self) {
        self.link.unlink();
    }
}

struct ListInner<T>
{
    head: Link<T>,
    tail: Link<T>,
}

pub struct List<T> {
    inner: UnsafeCell<ListInner<T>>
}

impl<T> List<T> {
    fn inner_mut(&self) -> &mut ListInner<T> {
        unsafe {
            &mut *self.inner.get()
        }
    }
    fn maybe_init_head_tail(&self) {
        unsafe {
            let inner = self.inner_mut();
            if inner.head.next.is_null() {
                inner.head.next = &mut inner.tail as *mut _;
                inner.tail.prev = &mut inner.head as *mut _;
            }
        }
    }
    pub const fn new() -> Self {
        Self { inner: UnsafeCell::new(ListInner {
            head: Link::new(),
            tail: Link::new(),
        }) }
    }

    #[allow(unused)]
    pub fn is_empty(&self) -> bool {
        self.maybe_init_head_tail();
        let inner = self.inner_mut();
        inner.head.next == &mut inner.tail
    }

    pub unsafe fn push_link_front(&mut self, link_owner: &mut T, link: *mut Link<T>) {
        self.maybe_init_head_tail();

        let link = &mut *link;
        let inner = self.inner_mut();
        link.owner = link_owner as *mut T;

        // We want to insert X at the front of list head--A--B--C--D--tail
        //
        // Set A.prev to X
        (&mut *inner.head.next).prev = link;
        // Set X.next to A
        link.next = inner.head.next;
        // Set X.prev to head
        link.prev = &mut inner.head as *mut _;
        // Set head.next to X
        inner.head.next = link;

        // List is now head--X--A--B--C--D--tail
    }

    pub unsafe fn push_link_back(&mut self, link_owner: &mut T, link: *mut Link<T>) {
        self.maybe_init_head_tail();
        let link = &mut *link;
        let inner = self.inner_mut();
        link.owner = link_owner as *mut T;

        // We want to insert X at the back of list head--A--B--C--D--tail
        //
        // Set D.next to X
        (&mut *inner.tail.prev).next = link;
        // Set X.next to tail
        link.next = &mut inner.tail;
        // Set X.prev to D
        link.prev = inner.tail.prev;
        // Set tail.prev to X
        inner.tail.prev = link;
        // List is now head--A--B--C--D--X--tail
    }

    pub fn push_node_front(&mut self, node: &mut Node<T>) {
        let link = &mut node.link as *mut Link<T>;
        unsafe { self.push_link_front(&mut node.data, link) };
    }

    pub fn push_node_back(&mut self, node: &mut Node<T>) {
        let link = &mut node.link as *mut Link<T>;
        unsafe { self.push_link_back(&mut node.data, link) };
    }

    pub fn pop_front(&mut self) -> Option<&mut Link<T>> {
        self.maybe_init_head_tail();
        unsafe {
            if self.is_empty() {
                None
            }
            else {
                // We want to remove A from head--A--B--C--tail
                let inner = self.inner_mut();
                let front = &mut *inner.head.next;
                front.unlink();
                Some(front)
            }
        }
    }

    /// Moves all nodes from one list to the front of another
    pub fn move_to_front_of(&mut self, dst: &mut Self) {
        self.maybe_init_head_tail();
        dst.maybe_init_head_tail();
        if !self.is_empty() {
            unsafe {
                let dst_inner = dst.inner_mut();
                let dst_front = &mut *dst.inner_mut().head.next;
                let inner = self.inner_mut();
                // Move list head--A--B--tail to the front of head2--C--D--tail2

                // Set C.prev to B
                dst_front.prev = inner.tail.prev;
                // Set B.next to C
                (&mut *inner.tail.prev).next = dst_front;
                // Set head2.next to A
                dst_inner.head.next = inner.head.next;
                // Set A.prev to head2
                (&mut *inner.head.next).prev = &mut dst_inner.head;

                inner.head.next = &mut inner.tail;
                inner.tail.prev = &mut inner.head;
            }
        }
    }

    pub unsafe fn iter(&self) -> ListIterator<T> {
        self.maybe_init_head_tail();
        ListIterator {
            current: Link{
                owner: core::ptr::null_mut(),
                next: self.inner_mut().head.next,
                prev: core::ptr::null_mut(),
            },
            tail: &mut self.inner_mut().tail,
            phantom: PhantomData,
        }
    }
}

pub struct ListIterator<'a, T: 'static> {
    current: Link<T>,
    tail: *mut Link<T>,
    phantom: core::marker::PhantomData<&'a ()>,
}

impl<'a, T: 'static> Iterator for ListIterator<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current.next != self.tail {
            unsafe {
                let next = &*self.current.next;
                self.current.next = next.next;
                Some(&*next.owner)
            }
        }
        else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_link_has_no_owner() {
        let link = Link::<i32>::new();
        assert_eq!(link.owner, core::ptr::null_mut());
    }
    #[test]
    fn new_list_is_empty() {
        let list = List::<i32>::new();
        assert!(list.is_empty());
    }
    #[test]
    fn push_to_empty() {
        unsafe {
            let mut link_item = 10;
            let mut list = List::<i32>::new();
            let mut link = Link::<i32>::new();

            list.push_link_front(&mut link_item, &mut link);
            assert_eq!(link.owner, &mut link_item as *mut _);

            assert!(!list.is_empty());
            assert_eq!(link.next, &mut list.inner_mut().tail as *mut _);
            assert_eq!(list.inner_mut().tail.prev, &mut link as *mut _);
            assert_eq!(list.inner_mut().head.next, &mut link as *mut _);
        }
    }

    #[test]
    fn push_front() {
        unsafe {
            let mut link_item = 10;
            let mut list = List::<i32>::new();
            let mut link1 = Link::<i32>::new();
            let mut link2 = Link::<i32>::new();
            let mut link3 = Link::<i32>::new();
            let mut link4 = Link::<i32>::new();

            list.push_link_front(&mut link_item, &mut link1);
            list.push_link_front(&mut link_item, &mut link2);
            list.push_link_front(&mut link_item, &mut link3);
            list.push_link_front(&mut link_item, &mut link4);

            assert_eq!(list.inner_mut().head.next, &mut link4 as *mut _);
            assert_eq!(link4.next, &mut link3 as *mut _);
            assert_eq!(link3.next, &mut link2 as *mut _);
            assert_eq!(link2.next, &mut link1 as *mut _);
            assert_eq!(link1.next, &mut list.inner_mut().tail as *mut _);
        }
    }

    #[test]
    fn push_back() {
        unsafe {
            let mut link_item = 10;
            let mut list = List::<i32>::new();
            let mut link1 = Link::<i32>::new();
            let mut link2 = Link::<i32>::new();
            let mut link3 = Link::<i32>::new();
            let mut link4 = Link::<i32>::new();

            list.push_link_back(&mut link_item, &mut link1);
            list.push_link_back(&mut link_item, &mut link2);
            list.push_link_back(&mut link_item, &mut link3);
            list.push_link_back(&mut link_item, &mut link4);

            assert_eq!(list.inner_mut().head.next, &mut link1 as *mut _);
            assert_eq!(link1.next, &mut link2 as *mut _);
            assert_eq!(link2.next, &mut link3 as *mut _);
            assert_eq!(link3.next, &mut link4 as *mut _);
            assert_eq!(link4.next, &mut list.inner_mut().tail as *mut _);
        }
    }

    #[test]
    fn pop_single_item() {
        unsafe {
            let mut link_item = 10;
            let mut list = List::<i32>::new();
            let mut link = Link::<i32>::new();

            list.push_link_front(&mut link_item, &mut link);
            assert!(!list.is_empty());
            let popped = list.pop_front();
            assert!(popped.is_some());
            assert_eq!(popped.unwrap().owner().unwrap(), &link_item);
            assert!(list.is_empty());
        }
    }

    #[test]
    fn pop_item() {
        unsafe {
            let mut link_item = 10;
            let mut list = List::<i32>::new();
            let mut link1 = Link::<i32>::new();
            let mut link2 = Link::<i32>::new();
            let mut link3 = Link::<i32>::new();
            let mut link4 = Link::<i32>::new();

            list.push_link_back(&mut link_item, &mut link1);
            list.push_link_back(&mut link_item, &mut link2);
            list.push_link_back(&mut link_item, &mut link3);
            list.push_link_back(&mut link_item, &mut link4);

            assert_eq!(list.inner_mut().head.next, &mut link1 as *mut _);
            assert_eq!(link1.next, &mut link2 as *mut _);
            assert_eq!(link2.next, &mut link3 as *mut _);
            assert_eq!(link3.next, &mut link4 as *mut _);
            assert_eq!(link4.next, &mut list.inner_mut().tail as *mut _);

            assert_eq!(list.pop_front().unwrap() as *mut Link<i32>, &mut link1 as *mut _);
            assert_eq!(list.inner_mut().head.next, &mut link2 as *mut _);
            assert_eq!(link2.next, &mut link3 as *mut _);
            assert_eq!(link3.next, &mut link4 as *mut _);
            assert_eq!(link4.next, &mut list.inner_mut().tail as *mut _);

            assert_eq!(link1.prev, core::ptr::null_mut());
            assert_eq!(link1.next, core::ptr::null_mut());
        }
    }

    #[test]
    fn move_to_empty() {
        unsafe {
            let mut link_item = 10;
            let mut list = List::<i32>::new();
            let mut list2 = List::<i32>::new();
            let mut link1 = Link::<i32>::new();
            let mut link2 = Link::<i32>::new();
            let mut link3 = Link::<i32>::new();
            let mut link4 = Link::<i32>::new();

            list.push_link_back(&mut link_item, &mut link1);
            list.push_link_back(&mut link_item, &mut link2);
            list2.push_link_back(&mut link_item, &mut link3);
            list2.push_link_back(&mut link_item, &mut link4);

            list.move_to_front_of(&mut list2);

            assert!(list.is_empty());
            assert!(!list2.is_empty());

            assert_eq!(list2.inner_mut().head.next, &mut link1 as *mut _);
            assert_eq!(link1.prev, &mut list2.inner_mut().head as *mut _);
            assert_eq!(link1.next, &mut link2 as *mut _);
            assert_eq!(link2.next, &mut link3 as *mut _);
            assert_eq!(link3.next, &mut link4 as *mut _);
            assert_eq!(link4.next, &mut list2.inner_mut().tail as *mut _);
        }
    }

    #[test]
    fn move_to_existing() {
        unsafe {
            let mut link_item = 10;
            let mut list = List::<i32>::new();
            let mut link = Link::<i32>::new();

            list.push_link_front(&mut link_item, &mut link);
            assert!(!list.is_empty());
            let mut list2 = List::<i32>::new();
            list.move_to_front_of(&mut list2);
            assert!(!list2.is_empty());
            assert!(list.is_empty());
            assert_eq!(list2.inner_mut().head.next, &mut link as *mut _);
            assert_eq!(link.prev, &mut list2.inner_mut().head as *mut _);
        }
    }

    #[test]
    fn remove_head_item() {
        unsafe {
            let mut link_item = 10;
            let mut list = List::<i32>::new();
            let mut link1 = Link::<i32>::new();
            let mut link2 = Link::<i32>::new();
            let mut link3 = Link::<i32>::new();
            let mut link4 = Link::<i32>::new();

            list.push_link_front(&mut link_item, &mut link1);
            list.push_link_front(&mut link_item, &mut link2);
            list.push_link_front(&mut link_item, &mut link3);
            list.push_link_front(&mut link_item, &mut link4);

            assert_eq!(list.inner_mut().head.next, &mut link4 as *mut _);
            assert_eq!(link4.next, &mut link3 as *mut _);
            assert_eq!(link3.next, &mut link2 as *mut _);
            assert_eq!(link2.next, &mut link1 as *mut _);
            assert_eq!(link1.next, &mut list.inner_mut().tail as *mut _);

            link4.unlink();

            assert_eq!(list.inner_mut().head.next, &mut link3 as *mut _);
            assert_eq!(link3.next, &mut link2 as *mut _);
            assert_eq!(link2.next, &mut link1 as *mut _);
            assert_eq!(link1.next, &mut list.inner_mut().tail as *mut _);
        }
    }

    #[test]
    fn remove_next_to_head_item() {
        unsafe {
            let mut link_item = 10;
            let mut list = List::<i32>::new();
            let mut link1 = Link::<i32>::new();
            let mut link2 = Link::<i32>::new();
            let mut link3 = Link::<i32>::new();
            let mut link4 = Link::<i32>::new();

            list.push_link_front(&mut link_item, &mut link1);
            list.push_link_front(&mut link_item, &mut link2);
            list.push_link_front(&mut link_item, &mut link3);
            list.push_link_front(&mut link_item, &mut link4);

            assert_eq!(list.inner_mut().head.next, &mut link4 as *mut _);
            assert_eq!(link4.next, &mut link3 as *mut _);
            assert_eq!(link3.next, &mut link2 as *mut _);
            assert_eq!(link2.next, &mut link1 as *mut _);
            assert_eq!(link1.next, &mut list.inner_mut().tail as *mut _);

            link3.unlink();

            assert_eq!(list.inner_mut().head.next, &mut link4 as *mut _);
            assert_eq!(link4.next, &mut link2 as *mut _);
            assert_eq!(link2.next, &mut link1 as *mut _);
            assert_eq!(link1.next, &mut list.inner_mut().tail as *mut _);

            assert_eq!(link3.next, core::ptr::null_mut());
            assert_eq!(link3.prev, core::ptr::null_mut());
        }
    }

    #[test]
    fn remove_in_list_item() {
        unsafe {
            let mut link_item = 10;
            let mut list = List::<i32>::new();
            let mut link1 = Link::<i32>::new();
            let mut link2 = Link::<i32>::new();
            let mut link3 = Link::<i32>::new();
            let mut link4 = Link::<i32>::new();

            list.push_link_front(&mut link_item, &mut link1);
            list.push_link_front(&mut link_item, &mut link2);
            list.push_link_front(&mut link_item, &mut link3);
            list.push_link_front(&mut link_item, &mut link4);

            assert_eq!(list.inner_mut().head.next, &mut link4 as *mut _);
            assert_eq!(link4.next, &mut link3 as *mut _);
            assert_eq!(link3.next, &mut link2 as *mut _);
            assert_eq!(link2.next, &mut link1 as *mut _);
            assert_eq!(link1.next, &mut list.inner_mut().tail as *mut _);

            link2.unlink();

            assert_eq!(list.inner_mut().head.next, &mut link4 as *mut _);
            assert_eq!(link4.next, &mut link3 as *mut _);
            assert_eq!(link3.next, &mut link1 as *mut _);
            assert_eq!(link1.next, &mut list.inner_mut().tail as *mut _);

            assert_eq!(link2.next, core::ptr::null_mut());
        }
    }

    #[test]
    fn iter_test_push_front() {
        let mut link_item = [1usize, 2, 3, 4,];
        let mut list = List::<usize>::new();
        let mut link1 = Link::<usize>::new();
        let mut link2 = Link::<usize>::new();
        let mut link3 = Link::<usize>::new();
        let mut link4 = Link::<usize>::new();

        unsafe {
            list.push_link_front(&mut link_item[3], &mut link1);
            list.push_link_front(&mut link_item[2], &mut link2);
            list.push_link_front(&mut link_item[1], &mut link3);
            list.push_link_front(&mut link_item[0], &mut link4);

            for x in list.iter().enumerate() {
                assert_eq!(x.0+1, *x.1);
            }
        }
    }

    #[test]
    fn iter_test_push_back() {
        let mut link_item = [1usize, 2, 3, 4,];
        let mut list = List::<usize>::new();
        let mut link1 = Link::<usize>::new();
        let mut link2 = Link::<usize>::new();
        let mut link3 = Link::<usize>::new();
        let mut link4 = Link::<usize>::new();

        unsafe {
            list.push_link_back(&mut link_item[0], &mut link1);
            list.push_link_back(&mut link_item[1], &mut link2);
            list.push_link_back(&mut link_item[2], &mut link3);
            list.push_link_back(&mut link_item[3], &mut link4);

            for x in list.iter().enumerate() {
                assert_eq!(x.0+1, *x.1);
            }
        }
    }
}
