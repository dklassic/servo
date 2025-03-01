/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! A safe wrapper for DOM nodes that prevents layout from mutating the DOM, from letting DOM nodes
//! escape, and from generally doing anything that it isn't supposed to. This is accomplished via
//! a simple whitelist of allowed operations, along with some lifetime magic to prevent nodes from
//! escaping.
//!
//! As a security wrapper is only as good as its whitelist, be careful when adding operations to
//! this list. The cardinal rules are:
//!
//! 1. Layout is not allowed to mutate the DOM.
//!
//! 2. Layout is not allowed to see anything with `LayoutDom` in the name, because it could hang
//!    onto these objects and cause use-after-free.
//!
//! When implementing wrapper functions, be careful that you do not touch the borrow flags, or you
//! will race and cause spurious thread failure. (Note that I do not believe these races are
//! exploitable, but they'll result in brokenness nonetheless.)
//!
//! Rules of the road for this file:
//!
//! * Do not call any methods on DOM nodes without checking to see whether they use borrow flags.
//!
//!   o Instead of `get_attr()`, use `.get_attr_val_for_layout()`.
//!
//!   o Instead of `html_element_in_html_document()`, use
//!     `html_element_in_html_document_for_layout()`.

#![allow(unsafe_code)]

use atomic_refcell::{AtomicRef, AtomicRefMut};
use script_layout_interface::wrapper_traits::{
    LayoutNode, ThreadSafeLayoutElement, ThreadSafeLayoutNode,
};
use style::dom::{NodeInfo, TElement, TNode};
use style::selector_parser::RestyleDamage;
use style::values::computed::counters::ContentItem;
use style::values::generics::counters::Content;

use crate::data::{InnerLayoutData, LayoutData, LayoutDataFlags};

pub trait ThreadSafeLayoutNodeHelpers<'dom> {
    fn borrow_layout_data(self) -> Option<AtomicRef<'dom, InnerLayoutData>>;
    fn mutate_layout_data(self) -> Option<AtomicRefMut<'dom, InnerLayoutData>>;

    /// Returns the layout data flags for this node.
    fn flags(self) -> LayoutDataFlags;

    /// Adds the given flags to this node.
    fn insert_flags(self, new_flags: LayoutDataFlags);

    /// Removes the given flags from this node.
    fn remove_flags(self, flags: LayoutDataFlags);

    /// If this is a text node, generated content, or a form element, copies out
    /// its content. Otherwise, panics.
    ///
    /// FIXME(pcwalton): This might have too much copying and/or allocation. Profile this.
    fn text_content(&self) -> TextContent;

    /// The RestyleDamage from any restyling, or RestyleDamage::rebuild_and_reflow() if this
    /// is the first time layout is visiting this node. We implement this here, rather than
    /// with the rest of the wrapper layer, because we need layout code to determine whether
    /// layout has visited the node.
    fn restyle_damage(self) -> RestyleDamage;
}

impl<'dom, T> ThreadSafeLayoutNodeHelpers<'dom> for T
where
    T: ThreadSafeLayoutNode<'dom>,
{
    fn borrow_layout_data(self) -> Option<AtomicRef<'dom, InnerLayoutData>> {
        self.layout_data()
            .map(|data| data.downcast_ref::<LayoutData>().unwrap().0.borrow())
    }

    fn mutate_layout_data(self) -> Option<AtomicRefMut<'dom, InnerLayoutData>> {
        self.layout_data()
            .and_then(|data| data.downcast_ref::<LayoutData>())
            .map(|data| data.0.borrow_mut())
    }

    fn flags(self) -> LayoutDataFlags {
        self.borrow_layout_data().as_ref().unwrap().flags
    }

    fn insert_flags(self, new_flags: LayoutDataFlags) {
        self.mutate_layout_data().unwrap().flags.insert(new_flags);
    }

    fn remove_flags(self, flags: LayoutDataFlags) {
        self.mutate_layout_data().unwrap().flags.remove(flags);
    }

    fn text_content(&self) -> TextContent {
        if self.get_pseudo_element_type().is_replaced_content() {
            let style = self.as_element().unwrap().resolved_style();

            return TextContent::GeneratedContent(match style.as_ref().get_counters().content {
                Content::Items(ref value) => value.items.to_vec(),
                _ => vec![],
            });
        }

        TextContent::Text(self.node_text_content().into_owned().into_boxed_str())
    }

    fn restyle_damage(self) -> RestyleDamage {
        // We need the underlying node to potentially access the parent in the
        // case of text nodes. This is safe as long as we don't let the parent
        // escape and never access its descendants.
        let mut node = self.unsafe_get();

        // If this is a text node, use the parent element, since that's what
        // controls our style.
        if node.is_text_node() {
            node = node.parent_node().unwrap();
            debug_assert!(node.is_element());
        }

        let damage = {
            let (layout_data, style_data) = match (node.layout_data(), node.style_data()) {
                (Some(layout_data), Some(style_data)) => (layout_data, style_data),
                _ => panic!(
                    "could not get style and layout data for <{}>",
                    node.as_element().unwrap().local_name()
                ),
            };

            let layout_data = layout_data.downcast_ref::<LayoutData>().unwrap().0.borrow();
            if !layout_data
                .flags
                .contains(crate::data::LayoutDataFlags::HAS_BEEN_TRAVERSED)
            {
                // We're reflowing a node that was styled for the first time and
                // has never been visited by layout. Return rebuild_and_reflow,
                // because that's what the code expects.
                RestyleDamage::rebuild_and_reflow()
            } else {
                style_data.element_data.borrow().damage
            }
        };

        damage
    }
}

pub enum TextContent {
    Text(Box<str>),
    GeneratedContent(Vec<ContentItem>),
}

impl TextContent {
    pub fn is_empty(&self) -> bool {
        match *self {
            TextContent::Text(_) => false,
            TextContent::GeneratedContent(ref content) => content.is_empty(),
        }
    }
}
