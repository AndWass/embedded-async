pub(crate) struct Link<T> {
    owner: *mut T,
    next: *mut Link<T>,
}

impl<T> Link<T> {
    pub const fn new() -> Self {
        Self {
            owner: core::ptr::null_mut(),
            next: core::ptr::null_mut(),
        }
    }

    pub fn owner(&self) -> Option<&T> {
        unsafe { self.owner.as_ref() }
    }
}

pub(crate) struct List<T> {
    head: *mut Link<T>,
}

impl<T> List<T> {
    pub const fn new() -> Self {
        Self {
            head: core::ptr::null_mut(),
        }
    }

    pub fn head_ref(&mut self) -> Option<&mut Link<T>> {
        unsafe { self.head.as_mut() }
    }

    pub fn is_empty(&self) -> bool {
        self.head.is_null()
    }

    pub unsafe fn push(&mut self, link_owner: &mut T, link: *mut Link<T>) {
        let link = &mut *link;
        link.owner = link_owner as *mut T;
        link.next = self.head;
        self.head = link;
    }

    pub fn pop(&mut self) -> Option<&mut Link<T>> {
        if let Some(head) = unsafe { self.head.as_mut() } {
            self.head = head.next;
            head.next = core::ptr::null_mut();
            Some(head)
        } else {
            None
        }
    }

    pub fn remove(&mut self, link: &mut Link<T>) -> bool {
        let needle = link as *mut Link<T>;
        if self.head == needle {
            self.head = link.next;
            link.next = core::ptr::null_mut();
            true
        } else if let Some(head) = self.head_ref() {
            if head.next == needle {
                head.next = link.next;
                link.next = core::ptr::null_mut();
                true
            } else if head.next == core::ptr::null_mut() {
                false
            } else {
                let mut iter = head.next;
                loop {
                    let iter_ref = unsafe { &mut *iter };
                    if iter_ref.next == core::ptr::null_mut() {
                        break false;
                    } else if iter_ref.next == needle {
                        iter_ref.next = link.next;
                        link.next = core::ptr::null_mut();
                        break true;
                    } else {
                        iter = iter_ref.next;
                    }
                }
            }
        } else {
            false
        }
    }

    pub fn take(&mut self) -> Self {
        Self {
            head: core::mem::replace(&mut self.head, core::ptr::null_mut()),
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

            list.push(&mut link_item, &mut link);
            assert_eq!(link.owner, &mut link_item as *mut _);

            assert!(!list.is_empty());
            assert!(link.next.is_null());
            assert_eq!(list.head, &mut link as *mut _);
        }
    }

    #[test]
    fn pop_single_item() {
        unsafe {
            let mut link_item = 10;
            let mut list = List::<i32>::new();
            let mut link = Link::<i32>::new();

            list.push(&mut link_item, &mut link);
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

            list.push(&mut link_item, &mut link);
            assert!(!list.is_empty());
            let list2 = list.take();
            assert!(!list2.is_empty());
            assert!(list.is_empty());
            assert_eq!(list2.head, &mut link as *mut _);
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

            list.push(&mut link_item, &mut link1);
            list.push(&mut link_item, &mut link2);
            list.push(&mut link_item, &mut link3);
            list.push(&mut link_item, &mut link4);

            assert_eq!(list.head, &mut link4 as *mut _);
            assert_eq!(link4.next, &mut link3 as *mut _);
            assert_eq!(link3.next, &mut link2 as *mut _);
            assert_eq!(link2.next, &mut link1 as *mut _);
            assert_eq!(link1.next, core::ptr::null_mut());

            assert!(list.remove(&mut link4));
            assert!(!list.remove(&mut link4));

            assert_eq!(list.head, &mut link3 as *mut _);
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

            list.push(&mut link_item, &mut link1);
            list.push(&mut link_item, &mut link2);
            list.push(&mut link_item, &mut link3);
            list.push(&mut link_item, &mut link4);

            assert_eq!(list.head, &mut link4 as *mut _);
            assert_eq!(link4.next, &mut link3 as *mut _);
            assert_eq!(link3.next, &mut link2 as *mut _);
            assert_eq!(link2.next, &mut link1 as *mut _);
            assert_eq!(link1.next, core::ptr::null_mut());

            assert!(list.remove(&mut link3));
            assert_eq!(list.head, &mut link4 as *mut _);
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

            list.push(&mut link_item, &mut link1);
            list.push(&mut link_item, &mut link2);
            list.push(&mut link_item, &mut link3);
            list.push(&mut link_item, &mut link4);

            assert_eq!(list.head, &mut link4 as *mut _);
            assert_eq!(link4.next, &mut link3 as *mut _);
            assert_eq!(link3.next, &mut link2 as *mut _);
            assert_eq!(link2.next, &mut link1 as *mut _);
            assert_eq!(link1.next, core::ptr::null_mut());

            assert!(list.remove(&mut link2));
            assert_eq!(list.head, &mut link4 as *mut _);
            assert_eq!(link4.next, &mut link3 as *mut _);
            assert_eq!(link3.next, &mut link1 as *mut _);
            assert_eq!(link1.next, core::ptr::null_mut());

            assert_eq!(link2.next, core::ptr::null_mut());
        }
    }
}
