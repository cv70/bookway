// Definition for singly-linked list.
#[derive(PartialEq, Eq, Clone, Debug)]
pub struct ListNode {
  pub val: i32,
  pub next: Option<Box<ListNode>>
}

impl ListNode {
  #[inline]
  fn new(val: i32) -> Self {
    ListNode {
      next: None,
      val
    }
  }
}
impl Solution {
    pub fn add_two_numbers(l1: Option<Box<ListNode>>, l2: Option<Box<ListNode>>) -> Option<Box<ListNode>> {
        let mut total = 0;
        while let Some(v1) = l1 {
            total += v1.val;
        }
        while let Some(v2) = l2 {
            total += v2.val;
        }

        let mut head: Option<Box<ListNode>> = None;

        while total > 0 {
            let mut val = total % 10;
            let mut node = Some(Box::new(ListNode::new(val)));
            node.next = head;
            head = node;
            total = total / 10;
        }

        return head
    }
}
