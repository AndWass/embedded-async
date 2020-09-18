pub(crate) struct Link<T> {
    owner: *mut T,
    next: *mut Link<T>
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

    pub fn owner_mut(&self) -> Option<&mut T> {
        unsafe { self.owner.as_mut() }
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

    pub fn is_empty(&self) -> bool {
        self.head.is_null()
    }

    pub fn push(&mut self, link_owner: &mut T, link: &mut Link<T>) {
        link.owner = link_owner as *mut T;
        link.next = self.head;
        self.head = link as *mut _;
    }

    pub fn pop(&mut self) -> Option<&mut Link<T>> {
        if let Some(head) = unsafe { self.head.as_mut() } {
            self.head = head.next;
            head.next = core::ptr::null_mut();
            Some(head)
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
        let mut link = Link::<i32>::new();
        assert!(link.owner().is_none());
        assert!(link.owner_mut().is_none());
    }
    #[test]
    fn new_list_is_empty() {
        let list = List::<i32>::new();
        assert!(list.is_empty());
    }
    #[test]
    fn push_to_empty() {
        let mut link_item = 10;
        let mut list = List::<i32>::new();
        let mut link = Link::<i32>::new();

        list.push(&mut link_item, &mut link);
        assert_eq!(link.owner, &mut link_item as *mut _);

        assert!(!list.is_empty());
        assert!(link.next.is_null());
        assert_eq!(list.head, &mut link as *mut _);
    }
    #[test]
    fn pop_single_item() {
        let mut link_item = 10;
        let mut list = List::<i32>::new();
        let mut link = Link::<i32>::new();

        list.push(&mut link_item, &mut link);
        assert!(!list.is_empty());
        let popped = list.pop();
        assert!(popped.is_some());
        assert!(list.is_empty());
    }
    #[test]
    fn remove_single_item() {
        let mut link_item = 10;
        let mut list = List::<i32>::new();
        let mut link = Link::<i32>::new();

        list.push(&mut link_item, &mut link);
        assert!(!list.is_empty());
        assert!(list.remove(&mut link));
        assert!(list.is_empty());
    }
}
