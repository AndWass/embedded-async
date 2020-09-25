use core::marker::PhantomData;

pub(crate) struct Link<T> {
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

pub(crate) struct List<T> {
    head: Link<T>,
}

pub(crate) struct Node<T> {
    pub(crate) data: T,
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

impl<T> List<T> {
    pub const fn new() -> Self {
        Self { head: Link::new() }
    }

    #[allow(unused)]
    pub fn is_empty(&self) -> bool {
        self.head.next.is_null()
    }

    pub unsafe fn push_link(&mut self, link_owner: &mut T, link: *mut Link<T>) {
        let link = &mut *link;
        link.owner = link_owner as *mut T;
        link.next = self.head.next;
        link.prev = &mut self.head as *mut _;
        self.head.next = link;

        link.next.as_mut().and_then(|x| {
            x.prev = link;
            Some(())
        });
    }

    pub fn push_node(&mut self, node: &mut Node<T>) {
        let link = &mut node.link as *mut Link<T>;
        unsafe { self.push_link(&mut node.data, link) };
    }

    pub fn pop(&mut self) -> Option<&mut Link<T>> {
        unsafe {
            if let Some(x) = self.head.next.as_mut() {
                x.unlink();
                Some(x)
            } else {
                None
            }
        }
    }

    /// Moves all nodes from one list to another
    pub fn move_to(&mut self, dst: &mut Self) {
        dst.head.next = self.head.next;
        self.head.next = core::ptr::null_mut();

        unsafe {
            if let Some(x) = dst.head.next.as_mut() {
                x.prev = &mut dst.head;
            }
        }
    }

    pub unsafe fn iter(&self) -> ListIterator<T> {
        ListIterator {
            current: Link{
                owner: core::ptr::null_mut(),
                next: self.head.next,
                prev: core::ptr::null_mut(),
            },
            phantom: PhantomData,
        }
    }
}

pub struct ListIterator<'a, T: 'static> {
    current: Link<T>,
    phantom: core::marker::PhantomData<&'a ()>,
}

impl<'a, T: 'static> Iterator for ListIterator<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        if !self.current.next.is_null() {
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

            list.push_link(&mut link_item, &mut link);
            assert_eq!(link.owner, &mut link_item as *mut _);

            assert!(!list.is_empty());
            assert!(link.next.is_null());
            assert_eq!(list.head.next, &mut link as *mut _);
        }
    }

    #[test]
    fn pop_single_item() {
        unsafe {
            let mut link_item = 10;
            let mut list = List::<i32>::new();
            let mut link = Link::<i32>::new();

            list.push_link(&mut link_item, &mut link);
            assert!(!list.is_empty());
            let popped = list.pop();
            assert!(popped.is_some());
            assert!(list.is_empty());
        }
    }

    #[test]
    fn take_list() {
        unsafe {
            let mut link_item = 10;
            let mut list = List::<i32>::new();
            let mut link = Link::<i32>::new();

            list.push_link(&mut link_item, &mut link);
            assert!(!list.is_empty());
            let mut list2 = List::<i32>::new();
            list.move_to(&mut list2);
            assert!(!list2.is_empty());
            assert!(list.is_empty());
            assert_eq!(list2.head.next, &mut link as *mut _);
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

            list.push_link(&mut link_item, &mut link1);
            list.push_link(&mut link_item, &mut link2);
            list.push_link(&mut link_item, &mut link3);
            list.push_link(&mut link_item, &mut link4);

            assert_eq!(list.head.next, &mut link4 as *mut _);
            assert_eq!(link4.next, &mut link3 as *mut _);
            assert_eq!(link3.next, &mut link2 as *mut _);
            assert_eq!(link2.next, &mut link1 as *mut _);
            assert_eq!(link1.next, core::ptr::null_mut());

            link4.unlink();

            assert_eq!(list.head.next, &mut link3 as *mut _);
            assert_eq!(link3.next, &mut link2 as *mut _);
            assert_eq!(link2.next, &mut link1 as *mut _);
            assert_eq!(link1.next, core::ptr::null_mut());
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

            list.push_link(&mut link_item, &mut link1);
            list.push_link(&mut link_item, &mut link2);
            list.push_link(&mut link_item, &mut link3);
            list.push_link(&mut link_item, &mut link4);

            assert_eq!(list.head.next, &mut link4 as *mut _);
            assert_eq!(link4.next, &mut link3 as *mut _);
            assert_eq!(link3.next, &mut link2 as *mut _);
            assert_eq!(link2.next, &mut link1 as *mut _);
            assert_eq!(link1.next, core::ptr::null_mut());

            link3.unlink();

            assert_eq!(list.head.next, &mut link4 as *mut _);
            assert_eq!(link4.next, &mut link2 as *mut _);
            assert_eq!(link2.next, &mut link1 as *mut _);
            assert_eq!(link1.next, core::ptr::null_mut());
            assert_eq!(link3.next, core::ptr::null_mut());
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

            list.push_link(&mut link_item, &mut link1);
            list.push_link(&mut link_item, &mut link2);
            list.push_link(&mut link_item, &mut link3);
            list.push_link(&mut link_item, &mut link4);

            assert_eq!(list.head.next, &mut link4 as *mut _);
            assert_eq!(link4.next, &mut link3 as *mut _);
            assert_eq!(link3.next, &mut link2 as *mut _);
            assert_eq!(link2.next, &mut link1 as *mut _);
            assert_eq!(link1.next, core::ptr::null_mut());

            link2.unlink();

            assert_eq!(list.head.next, &mut link4 as *mut _);
            assert_eq!(link4.next, &mut link3 as *mut _);
            assert_eq!(link3.next, &mut link1 as *mut _);
            assert_eq!(link1.next, core::ptr::null_mut());

            assert_eq!(link2.next, core::ptr::null_mut());
        }
    }

    #[test]
    fn iter_test() {
        let mut link_item = [1usize, 2, 3, 4,];
        let mut list = List::<usize>::new();
        let mut link1 = Link::<usize>::new();
        let mut link2 = Link::<usize>::new();
        let mut link3 = Link::<usize>::new();
        let mut link4 = Link::<usize>::new();

        unsafe {
            list.push_link(&mut link_item[3], &mut link1);
            list.push_link(&mut link_item[2], &mut link2);
            list.push_link(&mut link_item[1], &mut link3);
            list.push_link(&mut link_item[0], &mut link4);

            for x in list.iter().enumerate() {
                assert_eq!(x.0+1, *x.1);
            }
        }
    }
}
