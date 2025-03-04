/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Servo's experimental layout system builds a tree of `Flow` and `Fragment` objects and solves
//! layout constraints to obtain positions and display attributes of tree nodes. Positions are
//! computed in several tree traversals driven by the fundamental data dependencies required by
//! inline and block layout.
//!
//! Flows are interior nodes in the layout tree and correspond closely to *flow contexts* in the
//! CSS specification. Flows are responsible for positioning their child flow contexts and
//! fragments. Flows have purpose-specific fields, such as auxiliary line structs, out-of-flow
//! child lists, and so on.
//!
//! Currently, the important types of flows are:
//!
//! * `BlockFlow`: A flow that establishes a block context. It has several child flows, each of
//!   which are positioned according to block formatting context rules (CSS block boxes). Block
//!   flows also contain a single box to represent their rendered borders, padding, etc.
//!   The BlockFlow at the root of the tree has special behavior: it stretches to the boundaries of
//!   the viewport.
//!
//! * `InlineFlow`: A flow that establishes an inline context. It has a flat list of child
//!   fragments/flows that are subject to inline layout and line breaking and structs to represent
//!   line breaks and mapping to CSS boxes, for the purpose of handling `getClientRects()` and
//!   similar methods.

use std::fmt;
use std::slice::IterMut;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use app_units::Au;
use base::print_tree::PrintTree;
use bitflags::bitflags;
use euclid::default::{Point2D, Rect, Size2D, Vector2D};
use log::debug;
use serde::Serialize;
use serde::ser::{SerializeStruct, Serializer};
use servo_geometry::{MaxRect, au_rect_to_f32_rect, f32_rect_to_au_rect};
use style::computed_values::overflow_x::T as StyleOverflow;
use style::computed_values::position::T as Position;
use style::computed_values::text_align::T as TextAlign;
use style::context::SharedStyleContext;
use style::logical_geometry::{LogicalRect, LogicalSize, WritingMode};
use style::properties::ComputedValues;
use style::selector_parser::RestyleDamage;
use style::servo::restyle_damage::ServoRestyleDamage;
use webrender_api::units::LayoutTransform;

use crate::block::{BlockFlow, FormattingContextType};
use crate::context::LayoutContext;
use crate::display_list::items::ClippingAndScrolling;
use crate::display_list::{
    DisplayListBuildState, StackingContextCollectionState, StackingContextId,
};
use crate::flex::FlexFlow;
use crate::floats::{ClearType, FloatKind, Floats, SpeculatedFloatPlacement};
use crate::flow_list::{FlowList, FlowListIterator, MutFlowListIterator};
use crate::flow_ref::{FlowRef, WeakFlowRef};
use crate::fragment::{CoordinateSystem, Fragment, FragmentBorderBoxIterator, Overflow};
use crate::inline::InlineFlow;
use crate::model::{CollapsibleMargins, IntrinsicISizes};
use crate::parallel::FlowParallelInfo;
use crate::table::TableFlow;
use crate::table_cell::TableCellFlow;
use crate::table_colgroup::TableColGroupFlow;
use crate::table_row::TableRowFlow;
use crate::table_rowgroup::TableRowGroupFlow;
use crate::table_wrapper::TableWrapperFlow;

/// This marker trait indicates that a type is a struct with `#[repr(C)]` whose first field
/// is of type `BaseFlow` or some type that also implements this trait.
///
/// # Safety
///
/// The memory representation of `BaseFlow` must be a prefix of the memory representation of types
/// implementing `HasBaseFlow`. If this isn't the case, calling [`GetBaseFlow::base`] or
/// [`GetBaseFlow::mut_base`] could lead to memory errors.
#[allow(unsafe_code)]
pub unsafe trait HasBaseFlow {}

/// Methods to get the `BaseFlow` from any `HasBaseFlow` type.
pub trait GetBaseFlow {
    fn base(&self) -> &BaseFlow;
    fn mut_base(&mut self) -> &mut BaseFlow;
}

impl<T: HasBaseFlow + ?Sized> GetBaseFlow for T {
    #[inline(always)]
    #[allow(unsafe_code)]
    fn base(&self) -> &BaseFlow {
        let ptr: *const Self = self;
        let ptr = ptr as *const BaseFlow;
        unsafe { &*ptr }
    }

    #[inline(always)]
    #[allow(unsafe_code)]
    fn mut_base(&mut self) -> &mut BaseFlow {
        let ptr: *mut Self = self;
        let ptr = ptr as *mut BaseFlow;
        unsafe { &mut *ptr }
    }
}

/// Virtual methods that make up a float context.
///
/// Note that virtual methods have a cost; we should not overuse them in Servo. Consider adding
/// methods to `ImmutableFlowUtils` or `MutableFlowUtils` before adding more methods here.
pub trait Flow: HasBaseFlow + fmt::Debug + Sync + Send + 'static {
    // RTTI
    //
    // TODO(pcwalton): Use Rust's RTTI, once that works.

    /// Returns the class of flow that this is.
    fn class(&self) -> FlowClass;

    /// If this is a block flow, returns the underlying object. Fails otherwise.
    fn as_block(&self) -> &BlockFlow {
        panic!("called as_block() on a non-block flow")
    }

    /// If this is a block flow, returns the underlying object, borrowed mutably. Fails otherwise.
    fn as_mut_block(&mut self) -> &mut BlockFlow {
        debug!("called as_mut_block() on a flow of type {:?}", self.class());
        panic!("called as_mut_block() on a non-block flow")
    }

    /// If this is a flex flow, returns the underlying object. Fails otherwise.
    fn as_flex(&self) -> &FlexFlow {
        panic!("called as_flex() on a non-flex flow")
    }

    /// If this is an inline flow, returns the underlying object. Fails otherwise.
    fn as_inline(&self) -> &InlineFlow {
        panic!("called as_inline() on a non-inline flow")
    }

    /// If this is an inline flow, returns the underlying object, borrowed mutably. Fails
    /// otherwise.
    fn as_mut_inline(&mut self) -> &mut InlineFlow {
        panic!("called as_mut_inline() on a non-inline flow")
    }

    /// If this is a table wrapper flow, returns the underlying object. Fails otherwise.
    fn as_table_wrapper(&self) -> &TableWrapperFlow {
        panic!("called as_table_wrapper() on a non-tablewrapper flow")
    }

    /// If this is a table flow, returns the underlying object, borrowed mutably. Fails otherwise.
    fn as_mut_table(&mut self) -> &mut TableFlow {
        panic!("called as_mut_table() on a non-table flow")
    }

    /// If this is a table flow, returns the underlying object. Fails otherwise.
    fn as_table(&self) -> &TableFlow {
        panic!("called as_table() on a non-table flow")
    }

    /// If this is a table colgroup flow, returns the underlying object, borrowed mutably. Fails
    /// otherwise.
    fn as_mut_table_colgroup(&mut self) -> &mut TableColGroupFlow {
        panic!("called as_mut_table_colgroup() on a non-tablecolgroup flow")
    }

    /// If this is a table colgroup flow, returns the underlying object. Fails
    /// otherwise.
    fn as_table_colgroup(&self) -> &TableColGroupFlow {
        panic!("called as_table_colgroup() on a non-tablecolgroup flow")
    }

    /// If this is a table rowgroup flow, returns the underlying object, borrowed mutably. Fails
    /// otherwise.
    fn as_mut_table_rowgroup(&mut self) -> &mut TableRowGroupFlow {
        panic!("called as_mut_table_rowgroup() on a non-tablerowgroup flow")
    }

    /// If this is a table rowgroup flow, returns the underlying object. Fails otherwise.
    fn as_table_rowgroup(&self) -> &TableRowGroupFlow {
        panic!("called as_table_rowgroup() on a non-tablerowgroup flow")
    }

    /// If this is a table row flow, returns the underlying object, borrowed mutably. Fails
    /// otherwise.
    fn as_mut_table_row(&mut self) -> &mut TableRowFlow {
        panic!("called as_mut_table_row() on a non-tablerow flow")
    }

    /// If this is a table row flow, returns the underlying object. Fails otherwise.
    fn as_table_row(&self) -> &TableRowFlow {
        panic!("called as_table_row() on a non-tablerow flow")
    }

    /// If this is a table cell flow, returns the underlying object, borrowed mutably. Fails
    /// otherwise.
    fn as_mut_table_cell(&mut self) -> &mut TableCellFlow {
        panic!("called as_mut_table_cell() on a non-tablecell flow")
    }

    /// If this is a table cell flow, returns the underlying object. Fails otherwise.
    fn as_table_cell(&self) -> &TableCellFlow {
        panic!("called as_table_cell() on a non-tablecell flow")
    }

    // Main methods

    /// Pass 1 of reflow: computes minimum and preferred inline-sizes.
    ///
    /// Recursively (bottom-up) determine the flow's minimum and preferred inline-sizes. When
    /// called on this flow, all child flows have had their minimum and preferred inline-sizes set.
    /// This function must decide minimum/preferred inline-sizes based on its children's inline-
    /// sizes and the dimensions of any boxes it is responsible for flowing.
    fn bubble_inline_sizes(&mut self) {
        panic!("bubble_inline_sizes not yet implemented")
    }

    /// Pass 2 of reflow: computes inline-size.
    fn assign_inline_sizes(&mut self, _ctx: &LayoutContext) {
        panic!("assign_inline_sizes not yet implemented")
    }

    /// Pass 3a of reflow: computes block-size.
    fn assign_block_size(&mut self, _ctx: &LayoutContext) {
        panic!("assign_block_size not yet implemented")
    }

    /// Like `assign_block_size`, but is recurses explicitly into descendants.
    /// Fit as much content as possible within `available_block_size`.
    /// If that’s not all of it, truncate the contents of `self`
    /// and return a new flow similar to `self` with the rest of the content.
    ///
    /// The default is to make a flow "atomic": it can not be fragmented.
    fn fragment(
        &mut self,
        layout_context: &LayoutContext,
        _fragmentation_context: Option<FragmentationContext>,
    ) -> Option<Arc<dyn Flow>> {
        fn recursive_assign_block_size<F: ?Sized + Flow + GetBaseFlow>(
            flow: &mut F,
            ctx: &LayoutContext,
        ) {
            for child in flow.mut_base().child_iter_mut() {
                recursive_assign_block_size(child, ctx)
            }
            flow.assign_block_size(ctx);
        }
        recursive_assign_block_size(self, layout_context);
        None
    }

    fn collect_stacking_contexts(&mut self, state: &mut StackingContextCollectionState);

    /// If this is a float, places it. The default implementation does nothing.
    fn place_float_if_applicable(&mut self) {}

    /// Assigns block-sizes in-order; or, if this is a float, places the float. The default
    /// implementation simply assigns block-sizes if this flow might have floats in. Returns true
    /// if it was determined that this child might have had floats in or false otherwise.
    ///
    /// `parent_thread_id` is the thread ID of the parent. This is used for the layout tinting
    /// debug mode; if the block size of this flow was determined by its parent, we should treat
    /// it as laid out by its parent.
    fn assign_block_size_for_inorder_child_if_necessary(
        &mut self,
        layout_context: &LayoutContext,
        parent_thread_id: u8,
        _content_box: LogicalRect<Au>,
    ) -> bool {
        let might_have_floats_in_or_out =
            self.base().might_have_floats_in() || self.base().might_have_floats_out();
        if might_have_floats_in_or_out {
            self.mut_base().thread_id = parent_thread_id;
            self.assign_block_size(layout_context);
            self.mut_base()
                .restyle_damage
                .remove(ServoRestyleDamage::REFLOW_OUT_OF_FLOW | ServoRestyleDamage::REFLOW);
        }
        might_have_floats_in_or_out
    }

    fn has_non_invertible_transform_or_zero_scale(&self) -> bool {
        if !self.class().is_block_like() ||
            self.as_block()
                .fragment
                .style
                .get_box()
                .transform
                .0
                .is_empty()
        {
            return false;
        }

        self.as_block()
            .fragment
            .has_non_invertible_transform_or_zero_scale()
    }

    fn get_overflow_in_parent_coordinates(&self) -> Overflow {
        // FIXME(#2795): Get the real container size.
        let container_size = Size2D::zero();
        let position = self
            .base()
            .position
            .to_physical(self.base().writing_mode, container_size);

        let mut overflow = self.base().overflow;

        match self.class() {
            FlowClass::Block | FlowClass::TableCaption | FlowClass::TableCell => {},
            _ => {
                overflow.translate(&position.origin.to_vector());
                return overflow;
            },
        }

        let border_box = self.as_block().fragment.stacking_relative_border_box(
            &self.base().stacking_relative_position,
            &self
                .base()
                .early_absolute_position_info
                .relative_containing_block_size,
            self.base()
                .early_absolute_position_info
                .relative_containing_block_mode,
            CoordinateSystem::Own,
        );
        if StyleOverflow::Visible != self.as_block().fragment.style.get_box().overflow_x {
            overflow.paint.origin.x = Au(0);
            overflow.paint.size.width = border_box.size.width;
            overflow.scroll.origin.x = Au(0);
            overflow.scroll.size.width = border_box.size.width;
        }
        if StyleOverflow::Visible != self.as_block().fragment.style.get_box().overflow_y {
            overflow.paint.origin.y = Au(0);
            overflow.paint.size.height = border_box.size.height;
            overflow.scroll.origin.y = Au(0);
            overflow.scroll.size.height = border_box.size.height;
        }

        if !self.as_block().fragment.establishes_stacking_context() ||
            self.as_block()
                .fragment
                .style
                .get_box()
                .transform
                .0
                .is_empty()
        {
            overflow.translate(&position.origin.to_vector());
            return overflow;
        }

        // TODO: Take into account 3d transforms, even though it's a fairly
        // uncommon case.
        let transform_2d = self
            .as_block()
            .fragment
            .transform_matrix(&position)
            .unwrap_or(LayoutTransform::identity())
            .to_2d()
            .to_untyped();
        let transformed_overflow = Overflow {
            paint: f32_rect_to_au_rect(
                transform_2d.outer_transformed_rect(&au_rect_to_f32_rect(overflow.paint)),
            ),
            scroll: f32_rect_to_au_rect(
                transform_2d.outer_transformed_rect(&au_rect_to_f32_rect(overflow.scroll)),
            ),
        };

        // TODO: We are taking the union of the overflow and transformed overflow here, which
        // happened implicitly in the previous version of this code. This will probably be
        // unnecessary once we are taking into account 3D transformations above.
        overflow.union(&transformed_overflow);

        overflow.translate(&position.origin.to_vector());
        overflow
    }

    ///
    /// CSS Section 11.1
    /// This is the union of rectangles of the flows for which we define the
    /// Containing Block.
    ///
    /// FIXME(pcwalton): This should not be a virtual method, but currently is due to a compiler
    /// bug ("the trait `Sized` is not implemented for `self`").
    ///
    /// Assumption: This is called in a bottom-up traversal, so kids' overflows have
    /// already been set.
    /// Assumption: Absolute descendants have had their overflow calculated.
    fn store_overflow(&mut self, _: &LayoutContext) {
        // Calculate overflow on a per-fragment basis.
        let mut overflow = self.compute_overflow();
        match self.class() {
            FlowClass::Block | FlowClass::TableCaption | FlowClass::TableCell => {
                for kid in self.mut_base().children.iter_mut() {
                    overflow.union(&kid.get_overflow_in_parent_coordinates());
                }
            },
            _ => {},
        }
        self.mut_base().overflow = overflow
    }

    /// Phase 4 of reflow: Compute the stacking-relative position (origin of the content box,
    /// in coordinates relative to the nearest ancestor stacking context).
    fn compute_stacking_relative_position(&mut self, _: &LayoutContext) {
        // The default implementation is a no-op.
    }

    /// Phase 5 of reflow: builds display lists.
    fn build_display_list(&mut self, state: &mut DisplayListBuildState);

    /// Returns the union of all overflow rects of all of this flow's fragments.
    fn compute_overflow(&self) -> Overflow;

    /// Iterates through border boxes of all of this flow's fragments.
    /// Level provides a zero based index indicating the current
    /// depth of the flow tree during fragment iteration.
    fn iterate_through_fragment_border_boxes(
        &self,
        iterator: &mut dyn FragmentBorderBoxIterator,
        level: i32,
        stacking_context_position: &Point2D<Au>,
    );

    /// Mutably iterates through fragments in this flow.
    fn mutate_fragments(&mut self, mutator: &mut dyn FnMut(&mut Fragment));

    /// Marks this flow as the root flow. The default implementation is a no-op.
    fn mark_as_root(&mut self) {
        debug!("called mark_as_root() on a flow of type {:?}", self.class());
        panic!("called mark_as_root() on an unhandled flow");
    }

    // Note that the following functions are mostly called using static method
    // dispatch, so it's ok to have them in this trait. Plus, they have
    // different behaviour for different types of Flow, so they can't go into
    // the Immutable / Mutable Flow Utils traits without additional casts.

    fn is_root(&self) -> bool {
        false
    }

    /// The 'position' property of this flow.
    fn positioning(&self) -> Position {
        Position::Static
    }

    /// Return true if this flow has position 'fixed'.
    fn is_fixed(&self) -> bool {
        self.positioning() == Position::Fixed
    }

    fn contains_positioned_fragments(&self) -> bool {
        self.contains_relatively_positioned_fragments() ||
            self.base()
                .flags
                .contains(FlowFlags::IS_ABSOLUTELY_POSITIONED)
    }

    fn contains_relatively_positioned_fragments(&self) -> bool {
        self.positioning() == Position::Relative
    }

    /// Returns true if this is an absolute containing block.
    fn is_absolute_containing_block(&self) -> bool {
        self.contains_positioned_fragments()
    }

    /// Returns true if this flow contains fragments that are roots of an absolute flow tree.
    fn contains_roots_of_absolute_flow_tree(&self) -> bool {
        self.contains_relatively_positioned_fragments() || self.is_root()
    }

    /// Updates the inline position of a child flow during the assign-height traversal. At present,
    /// this is only used for absolutely-positioned inline-blocks.
    fn update_late_computed_inline_position_if_necessary(&mut self, inline_position: Au);

    /// Updates the block position of a child flow during the assign-height traversal. At present,
    /// this is only used for absolutely-positioned inline-blocks.
    fn update_late_computed_block_position_if_necessary(&mut self, block_position: Au);

    /// Return the size of the containing block generated by this flow for the absolutely-
    /// positioned descendant referenced by `for_flow`. For block flows, this is the padding box.
    ///
    /// NB: Do not change this `&self` to `&mut self` under any circumstances! It has security
    /// implications because this can be called on parents concurrently from descendants!
    fn generated_containing_block_size(&self, _: OpaqueFlow) -> LogicalSize<Au>;

    /// Attempts to perform incremental fixup of this flow by replacing its fragment's style with
    /// the new style. This can only succeed if the flow has exactly one fragment.
    fn repair_style(&mut self, new_style: &crate::ServoArc<ComputedValues>);

    /// Print any extra children (such as fragments) contained in this Flow
    /// for debugging purposes. Any items inserted into the tree will become
    /// children of this flow.
    fn print_extra_flow_children(&self, _: &mut PrintTree) {}

    fn clipping_and_scrolling(&self) -> ClippingAndScrolling {
        match self.base().clipping_and_scrolling {
            Some(info) => info,
            None => unreachable!("Tried to access scroll root id on Flow before assignment"),
        }
    }
}

#[allow(clippy::wrong_self_convention)]
pub trait ImmutableFlowUtils {
    // Convenience functions

    /// Returns true if this flow is a block flow or subclass thereof.
    fn is_block_like(self) -> bool;

    /// Returns true if this flow is a table flow.
    fn is_table(self) -> bool;

    /// Returns true if this flow is a table caption flow.
    fn is_table_caption(self) -> bool;

    /// Returns true if this flow is a table row flow.
    fn is_table_row(self) -> bool;

    /// Returns true if this flow is a table cell flow.
    fn is_table_cell(self) -> bool;

    /// Returns true if this flow is a table colgroup flow.
    fn is_table_colgroup(self) -> bool;

    /// Returns true if this flow is a table rowgroup flow.
    fn is_table_rowgroup(self) -> bool;

    /// Returns the number of children that this flow possesses.
    fn child_count(self) -> usize;

    /// Returns true if this flow is a block flow.
    fn is_block_flow(self) -> bool;

    /// Returns true if this flow is an inline flow.
    fn is_inline_flow(self) -> bool;

    /// Dumps the flow tree for debugging.
    fn print(self, title: String);

    /// Dumps the flow tree for debugging into the given PrintTree.
    fn print_with_tree(self, print_tree: &mut PrintTree);

    /// Returns true if floats might flow through this flow, as determined by the float placement
    /// speculation pass.
    fn floats_might_flow_through(self) -> bool;

    fn baseline_offset_of_last_line_box_in_flow(self) -> Option<Au>;
}

pub trait MutableFlowUtils {
    /// Calls `repair_style` and `bubble_inline_sizes`. You should use this method instead of
    /// calling them individually, since there is no reason not to perform both operations.
    fn repair_style_and_bubble_inline_sizes(self, style: &crate::ServoArc<ComputedValues>);
}

pub trait MutableOwnedFlowUtils {
    /// Set absolute descendants for this flow.
    ///
    /// Set this flow as the Containing Block for all the absolute descendants.
    fn set_absolute_descendants(&mut self, abs_descendants: AbsoluteDescendants);

    /// Push absolute descendants to this flow.
    ///
    /// Set this flow as the Containing Block for the provided absolute descendants.
    fn push_absolute_descendants(&mut self, abs_descendants: AbsoluteDescendants);

    /// Sets the flow as the containing block for all absolute descendants that have been marked
    /// as having reached their containing block. This is needed in order to handle cases like:
    ///
    /// ```html
    /// <div>
    ///     <span style="position: relative">
    ///         <span style="position: absolute; ..."></span>
    ///     </span>
    /// </div>
    /// ```
    fn take_applicable_absolute_descendants(
        &mut self,
        absolute_descendants: &mut AbsoluteDescendants,
    );
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub enum FlowClass {
    Block,
    Inline,
    ListItem,
    TableWrapper,
    Table,
    TableColGroup,
    TableRowGroup,
    TableRow,
    TableCaption,
    TableCell,
    Multicol,
    MulticolColumn,
    Flex,
}

impl FlowClass {
    fn is_block_like(self) -> bool {
        matches!(
            self,
            FlowClass::Block |
                FlowClass::ListItem |
                FlowClass::Table |
                FlowClass::TableRowGroup |
                FlowClass::TableRow |
                FlowClass::TableCaption |
                FlowClass::TableCell |
                FlowClass::TableWrapper |
                FlowClass::Flex
        )
    }
}

bitflags! {
    /// Flags used in flows.
    #[derive(Clone, Copy, Debug)]
    pub struct FlowFlags: u32 {
        /// Whether this flow is absolutely positioned. This is checked all over layout, so a
        /// virtual call is too expensive.
        const IS_ABSOLUTELY_POSITIONED = 0b0000_0000_0000_0000_0100_0000;
        /// Whether this flow clears to the left. This is checked all over layout, so a
        /// virtual call is too expensive.
        const CLEARS_LEFT = 0b0000_0000_0000_0000_1000_0000;
        /// Whether this flow clears to the right. This is checked all over layout, so a
        /// virtual call is too expensive.
        const CLEARS_RIGHT = 0b0000_0000_0000_0001_0000_0000;
        /// Whether this flow is left-floated. This is checked all over layout, so a
        /// virtual call is too expensive.
        const FLOATS_LEFT = 0b0000_0000_0000_0010_0000_0000;
        /// Whether this flow is right-floated. This is checked all over layout, so a
        /// virtual call is too expensive.
        const FLOATS_RIGHT = 0b0000_0000_0000_0100_0000_0000;
        /// Whether this flow has a fragment with `counter-reset` or `counter-increment`
        /// styles.
        const AFFECTS_COUNTERS = 0b0000_0000_0000_1000_0000_0000;
        /// Whether this flow's descendants have fragments that affect `counter-reset` or
        //  `counter-increment` styles.
        const HAS_COUNTER_AFFECTING_CHILDREN = 0b0000_0000_0001_0000_0000_0000;
        /// Whether this flow behaves as though it had `position: static` for the purposes
        /// of positioning in the inline direction. This is set for flows with `position:
        /// static` and `position: relative` as well as absolutely-positioned flows with
        /// unconstrained positions in the inline direction."]
        const INLINE_POSITION_IS_STATIC = 0b0000_0000_0010_0000_0000_0000;
        /// Whether this flow behaves as though it had `position: static` for the purposes
        /// of positioning in the block direction. This is set for flows with `position:
        /// static` and `position: relative` as well as absolutely-positioned flows with
        /// unconstrained positions in the block direction.
        const BLOCK_POSITION_IS_STATIC = 0b0000_0000_0100_0000_0000_0000;

        /// Whether any ancestor is a fragmentation container
        const CAN_BE_FRAGMENTED = 0b0000_0000_1000_0000_0000_0000;

        /// Whether this flow contains any text and/or replaced fragments.
        const CONTAINS_TEXT_OR_REPLACED_FRAGMENTS = 0b0000_0001_0000_0000_0000_0000;

        /// Whether margins are prohibited from collapsing with this flow.
        const MARGINS_CANNOT_COLLAPSE = 0b0000_0010_0000_0000_0000_0000;
    }
}

impl FlowFlags {
    #[inline]
    pub fn float_kind(&self) -> Option<FloatKind> {
        if self.contains(FlowFlags::FLOATS_LEFT) {
            Some(FloatKind::Left)
        } else if self.contains(FlowFlags::FLOATS_RIGHT) {
            Some(FloatKind::Right)
        } else {
            None
        }
    }

    #[inline]
    pub fn is_float(&self) -> bool {
        self.contains(FlowFlags::FLOATS_LEFT) || self.contains(FlowFlags::FLOATS_RIGHT)
    }

    #[inline]
    pub fn clears_floats(&self) -> bool {
        self.contains(FlowFlags::CLEARS_LEFT) || self.contains(FlowFlags::CLEARS_RIGHT)
    }
}

/// Absolutely-positioned descendants of this flow.
#[derive(Clone)]
pub struct AbsoluteDescendants {
    /// Links to every descendant. This must be private because it is unsafe to leak `FlowRef`s to
    /// layout.
    descendant_links: Vec<AbsoluteDescendantInfo>,
}

impl AbsoluteDescendants {
    pub fn new() -> AbsoluteDescendants {
        AbsoluteDescendants {
            descendant_links: Vec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.descendant_links.len()
    }

    pub fn is_empty(&self) -> bool {
        self.descendant_links.is_empty()
    }

    pub fn push(&mut self, given_descendant: FlowRef) {
        self.descendant_links.push(AbsoluteDescendantInfo {
            flow: given_descendant,
            has_reached_containing_block: false,
        });
    }

    /// Push the given descendants on to the existing descendants.
    ///
    /// Ignore any static y offsets, because they are None before layout.
    pub fn push_descendants(&mut self, given_descendants: AbsoluteDescendants) {
        for elem in given_descendants.descendant_links {
            self.descendant_links.push(elem);
        }
    }

    /// Return an iterator over the descendant flows.
    pub fn iter(&mut self) -> AbsoluteDescendantIter {
        AbsoluteDescendantIter {
            iter: self.descendant_links.iter_mut(),
        }
    }

    /// Mark these descendants as having reached their containing block.
    pub fn mark_as_having_reached_containing_block(&mut self) {
        for descendant_info in self.descendant_links.iter_mut() {
            descendant_info.has_reached_containing_block = true
        }
    }
}

impl Default for AbsoluteDescendants {
    fn default() -> Self {
        Self::new()
    }
}

/// Information about each absolutely-positioned descendant of the given flow.
#[derive(Clone)]
pub struct AbsoluteDescendantInfo {
    /// The absolute descendant flow in question.
    flow: FlowRef,

    /// Whether the absolute descendant has reached its containing block. This exists so that we
    /// can handle cases like the following:
    ///
    /// ```html
    /// <div>
    ///     <span id=a style="position: absolute; ...">foo</span>
    ///     <span style="position: relative">
    ///         <span id=b style="position: absolute; ...">bar</span>
    ///     </span>
    /// </div>
    /// ```
    ///
    /// When we go to create the `InlineFlow` for the outer `div`, our absolute descendants will
    /// be `a` and `b`. At this point, we need a way to distinguish between the two, because the
    /// containing block for `a` will be different from the containing block for `b`. Specifically,
    /// the latter's containing block is the inline flow itself, while the former's containing
    /// block is going to be some parent of the outer `div`. Hence we need this flag as a way to
    /// distinguish the two; it will be false for `a` and true for `b`.
    has_reached_containing_block: bool,
}

pub struct AbsoluteDescendantIter<'a> {
    iter: IterMut<'a, AbsoluteDescendantInfo>,
}

impl<'a> Iterator for AbsoluteDescendantIter<'a> {
    type Item = &'a mut dyn Flow;
    fn next(&mut self) -> Option<&'a mut dyn Flow> {
        self.iter
            .next()
            .map(|info| FlowRef::deref_mut(&mut info.flow))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

/// Information needed to compute absolute (i.e. viewport-relative) flow positions (not to be
/// confused with absolutely-positioned flows) that is computed during block-size assignment.
#[derive(Clone, Copy)]
pub struct EarlyAbsolutePositionInfo {
    /// The size of the containing block for relatively-positioned descendants.
    pub relative_containing_block_size: LogicalSize<Au>,

    /// The writing mode for `relative_containing_block_size`.
    pub relative_containing_block_mode: WritingMode,
}

impl EarlyAbsolutePositionInfo {
    pub fn new(writing_mode: WritingMode) -> EarlyAbsolutePositionInfo {
        // FIXME(pcwalton): The initial relative containing block-size should be equal to the size
        // of the root layer.
        EarlyAbsolutePositionInfo {
            relative_containing_block_size: LogicalSize::zero(writing_mode),
            relative_containing_block_mode: writing_mode,
        }
    }
}

/// Information needed to compute absolute (i.e. viewport-relative) flow positions (not to be
/// confused with absolutely-positioned flows) that is computed during final position assignment.
#[derive(Clone, Copy, Serialize)]
pub struct LateAbsolutePositionInfo {
    /// The position of the absolute containing block relative to the nearest ancestor stacking
    /// context. If the absolute containing block establishes the stacking context for this flow,
    /// and this flow is not itself absolutely-positioned, then this is (0, 0).
    pub stacking_relative_position_of_absolute_containing_block: Point2D<Au>,
}

impl LateAbsolutePositionInfo {
    pub fn new() -> LateAbsolutePositionInfo {
        LateAbsolutePositionInfo {
            stacking_relative_position_of_absolute_containing_block: Point2D::zero(),
        }
    }
}

impl Default for LateAbsolutePositionInfo {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FragmentationContext {
    pub available_block_size: Au,
    pub this_fragment_is_empty: bool,
}

/// Data common to all flows.
pub struct BaseFlow {
    pub restyle_damage: RestyleDamage,

    /// The children of this flow.
    pub children: FlowList,

    /// Intrinsic inline sizes for this flow.
    pub intrinsic_inline_sizes: IntrinsicISizes,

    /// The upper left corner of the box representing this flow, relative to the box representing
    /// its parent flow.
    ///
    /// For absolute flows, this represents the position with respect to its *containing block*.
    ///
    /// This does not include margins in the block flow direction, because those can collapse. So
    /// for the block direction (usually vertical), this represents the *border box*. For the
    /// inline direction (usually horizontal), this represents the *margin box*.
    pub position: LogicalRect<Au>,

    /// The amount of overflow of this flow, relative to the containing block. Must include all the
    /// pixels of all the display list items for correct invalidation.
    pub overflow: Overflow,

    /// Data used during parallel traversals.
    ///
    /// TODO(pcwalton): Group with other transient data to save space.
    pub parallel: FlowParallelInfo,

    /// The floats next to this flow.
    pub floats: Floats,

    /// Metrics for floats in computed during the float metrics speculation phase.
    pub speculated_float_placement_in: SpeculatedFloatPlacement,

    /// Metrics for floats out computed during the float metrics speculation phase.
    pub speculated_float_placement_out: SpeculatedFloatPlacement,

    /// The collapsible margins for this flow, if any.
    pub collapsible_margins: CollapsibleMargins,

    /// The position of this flow relative to the start of the nearest ancestor stacking context.
    /// This is computed during the top-down pass of display list construction.
    pub stacking_relative_position: Vector2D<Au>,

    /// Details about descendants with position 'absolute' or 'fixed' for which we are the
    /// containing block. This is in tree order. This includes any direct children.
    pub abs_descendants: AbsoluteDescendants,

    /// The inline-size of the block container of this flow. Used for computing percentage and
    /// automatic values for `width`.
    pub block_container_inline_size: Au,

    /// The writing mode of the block container of this flow.
    ///
    /// FIXME (mbrubeck): Combine this and block_container_inline_size and maybe
    /// block_container_explicit_block_size into a struct, to guarantee they are set at the same
    /// time?  Or just store a link to the containing block flow.
    pub block_container_writing_mode: WritingMode,

    /// The block-size of the block container of this flow, if it is an explicit size (does not
    /// depend on content heights).  Used for computing percentage values for `height`.
    pub block_container_explicit_block_size: Option<Au>,

    /// Reference to the Containing Block, if this flow is absolutely positioned.
    pub absolute_cb: ContainingBlockLink,

    /// Information needed to compute absolute (i.e. viewport-relative) flow positions (not to be
    /// confused with absolutely-positioned flows) that is computed during block-size assignment.
    pub early_absolute_position_info: EarlyAbsolutePositionInfo,

    /// Information needed to compute absolute (i.e. viewport-relative) flow positions (not to be
    /// confused with absolutely-positioned flows) that is computed during final position
    /// assignment.
    pub late_absolute_position_info: LateAbsolutePositionInfo,

    /// The clipping rectangle for this flow and its descendants, in the coordinate system of the
    /// nearest ancestor stacking context. If this flow itself represents a stacking context, then
    /// this is in the flow's own coordinate system.
    pub clip: Rect<Au>,

    /// The writing mode for this flow.
    pub writing_mode: WritingMode,

    /// For debugging and profiling, the identifier of the thread that laid out this fragment.
    pub thread_id: u8,

    /// Various flags for flows, tightly packed to save space.
    pub flags: FlowFlags,

    /// Text alignment of this flow.
    pub text_align: TextAlign,

    /// The ID of the StackingContext that contains this flow. This is initialized
    /// to 0, but it assigned during the collect_stacking_contexts phase of display
    /// list construction.
    pub stacking_context_id: StackingContextId,

    /// The indices of this Flow's ClipScrollNode. This is used to place the node's
    /// display items into scrolling frames and clipping nodes.
    pub clipping_and_scrolling: Option<ClippingAndScrolling>,
}

impl fmt::Debug for BaseFlow {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let child_count = self.parallel.children_count.load(Ordering::SeqCst);
        let child_count_string = if child_count > 0 {
            format!("\nchildren={}", child_count)
        } else {
            "".to_owned()
        };

        let absolute_descendants_string = if !self.abs_descendants.is_empty() {
            format!("\nabs-descendents={}", self.abs_descendants.len())
        } else {
            "".to_owned()
        };

        let damage_string = if self.restyle_damage != RestyleDamage::empty() {
            format!("\ndamage={:?}", self.restyle_damage)
        } else {
            "".to_owned()
        };

        let flags_string = if !self.flags.is_empty() {
            format!("\nflags={:?}", self.flags)
        } else {
            "".to_owned()
        };

        write!(
            f,
            "\nsc={:?}\
             \npos={:?}{}{}\
             \nfloatspec-in={:?}\
             \nfloatspec-out={:?}\
             \noverflow={:?}\
             {child_count_string}\
             {absolute_descendants_string}\
             {damage_string}\
             {flags_string}",
            self.stacking_context_id,
            self.position,
            if self.flags.contains(FlowFlags::FLOATS_LEFT) {
                "FL"
            } else {
                ""
            },
            if self.flags.contains(FlowFlags::FLOATS_RIGHT) {
                "FR"
            } else {
                ""
            },
            self.speculated_float_placement_in,
            self.speculated_float_placement_out,
            self.overflow,
        )
    }
}

impl Serialize for BaseFlow {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut serializer = serializer.serialize_struct("base", 5)?;
        serializer.serialize_field(
            "stacking_relative_position",
            &self.stacking_relative_position,
        )?;
        serializer.serialize_field("intrinsic_inline_sizes", &self.intrinsic_inline_sizes)?;
        serializer.serialize_field("position", &self.position)?;
        serializer.serialize_field("children", &self.children)?;
        serializer.end()
    }
}

/// Whether a base flow should be forced to be nonfloated. This can affect e.g. `TableFlow`, which
/// is never floated because the table wrapper flow is the floated one.
#[derive(Clone, PartialEq)]
pub enum ForceNonfloatedFlag {
    /// The flow should be floated if the node has a `float` property.
    FloatIfNecessary,
    /// The flow should be forced to be nonfloated.
    ForceNonfloated,
}

impl BaseFlow {
    #[inline]
    pub fn new(
        style: Option<&ComputedValues>,
        writing_mode: WritingMode,
        force_nonfloated: ForceNonfloatedFlag,
    ) -> BaseFlow {
        let mut flags = FlowFlags::empty();
        match style {
            Some(style) => {
                if style.can_be_fragmented() {
                    flags.insert(FlowFlags::CAN_BE_FRAGMENTED);
                }

                match style.get_box().position {
                    Position::Absolute | Position::Fixed => {
                        flags.insert(FlowFlags::IS_ABSOLUTELY_POSITIONED);

                        let logical_position = style.logical_position();
                        if logical_position.inline_start.is_auto() &&
                            logical_position.inline_end.is_auto()
                        {
                            flags.insert(FlowFlags::INLINE_POSITION_IS_STATIC);
                        }
                        if logical_position.block_start.is_auto() &&
                            logical_position.block_end.is_auto()
                        {
                            flags.insert(FlowFlags::BLOCK_POSITION_IS_STATIC);
                        }
                    },
                    _ => flags.insert(
                        FlowFlags::BLOCK_POSITION_IS_STATIC | FlowFlags::INLINE_POSITION_IS_STATIC,
                    ),
                }

                if force_nonfloated == ForceNonfloatedFlag::FloatIfNecessary {
                    // FIXME: this should use the writing mode of the containing block.
                    match FloatKind::from_property_and_writing_mode(
                        style.get_box().float,
                        style.writing_mode,
                    ) {
                        None => {},
                        Some(FloatKind::Left) => flags.insert(FlowFlags::FLOATS_LEFT),
                        Some(FloatKind::Right) => flags.insert(FlowFlags::FLOATS_RIGHT),
                    }
                }

                match ClearType::from_style(style) {
                    None => {},
                    Some(ClearType::Left) => flags.insert(FlowFlags::CLEARS_LEFT),
                    Some(ClearType::Right) => flags.insert(FlowFlags::CLEARS_RIGHT),
                    Some(ClearType::Both) => {
                        flags.insert(FlowFlags::CLEARS_LEFT);
                        flags.insert(FlowFlags::CLEARS_RIGHT);
                    },
                }

                if !style.get_counters().counter_reset.is_empty() ||
                    !style.get_counters().counter_increment.is_empty()
                {
                    flags.insert(FlowFlags::AFFECTS_COUNTERS)
                }
            },
            None => flags
                .insert(FlowFlags::BLOCK_POSITION_IS_STATIC | FlowFlags::INLINE_POSITION_IS_STATIC),
        }

        // New flows start out as fully damaged.
        let mut damage = RestyleDamage::rebuild_and_reflow();
        damage.remove(ServoRestyleDamage::RECONSTRUCT_FLOW);

        BaseFlow {
            restyle_damage: damage,
            children: FlowList::new(),
            intrinsic_inline_sizes: IntrinsicISizes::new(),
            position: LogicalRect::zero(writing_mode),
            overflow: Overflow::new(),
            parallel: FlowParallelInfo::new(),
            floats: Floats::new(writing_mode),
            collapsible_margins: CollapsibleMargins::new(),
            stacking_relative_position: Vector2D::zero(),
            abs_descendants: AbsoluteDescendants::new(),
            speculated_float_placement_in: SpeculatedFloatPlacement::zero(),
            speculated_float_placement_out: SpeculatedFloatPlacement::zero(),
            block_container_inline_size: Au(0),
            block_container_writing_mode: writing_mode,
            block_container_explicit_block_size: None,
            absolute_cb: ContainingBlockLink::new(),
            early_absolute_position_info: EarlyAbsolutePositionInfo::new(writing_mode),
            late_absolute_position_info: LateAbsolutePositionInfo::new(),
            clip: MaxRect::max_rect(),
            flags,
            text_align: TextAlign::Start,
            writing_mode,
            thread_id: 0,
            stacking_context_id: StackingContextId::root(),
            clipping_and_scrolling: None,
        }
    }

    /// Update the 'flags' field when computed styles have changed.
    ///
    /// These flags are initially set during flow construction.  They only need to be updated here
    /// if they are based on properties that can change without triggering `RECONSTRUCT_FLOW`.
    pub fn update_flags_if_needed(&mut self, style: &ComputedValues) {
        // For absolutely-positioned flows, changes to top/bottom/left/right can cause these flags
        // to get out of date:
        if self
            .restyle_damage
            .contains(ServoRestyleDamage::REFLOW_OUT_OF_FLOW)
        {
            // Note: We don't need to check whether IS_ABSOLUTELY_POSITIONED has changed, because
            // changes to the 'position' property trigger flow reconstruction.
            if self.flags.contains(FlowFlags::IS_ABSOLUTELY_POSITIONED) {
                let logical_position = style.logical_position();
                self.flags.set(
                    FlowFlags::INLINE_POSITION_IS_STATIC,
                    logical_position.inline_start.is_auto() &&
                        logical_position.inline_end.is_auto(),
                );
                self.flags.set(
                    FlowFlags::BLOCK_POSITION_IS_STATIC,
                    logical_position.block_start.is_auto() && logical_position.block_end.is_auto(),
                );
            }
        }
    }

    /// Return a new BaseFlow like this one but with the given children list
    pub fn clone_with_children(&self, children: FlowList) -> BaseFlow {
        BaseFlow {
            children,
            restyle_damage: self.restyle_damage |
                ServoRestyleDamage::REPAINT |
                ServoRestyleDamage::REFLOW_OUT_OF_FLOW |
                ServoRestyleDamage::REFLOW,
            parallel: FlowParallelInfo::new(),
            floats: self.floats.clone(),
            abs_descendants: self.abs_descendants.clone(),
            absolute_cb: self.absolute_cb.clone(),
            clip: self.clip,

            ..*self
        }
    }

    /// Iterates over the children of this immutable flow.
    pub fn child_iter(&self) -> FlowListIterator {
        self.children.iter()
    }

    pub fn child_iter_mut(&mut self) -> MutFlowListIterator {
        self.children.iter_mut()
    }

    pub fn collect_stacking_contexts_for_children(
        &mut self,
        state: &mut StackingContextCollectionState,
    ) {
        for kid in self.children.iter_mut() {
            if !kid.has_non_invertible_transform_or_zero_scale() {
                kid.collect_stacking_contexts(state);
            }
        }
    }

    #[inline]
    pub fn might_have_floats_in(&self) -> bool {
        self.speculated_float_placement_in.left > Au(0) ||
            self.speculated_float_placement_in.right > Au(0)
    }

    #[inline]
    pub fn might_have_floats_out(&self) -> bool {
        self.speculated_float_placement_out.left > Au(0) ||
            self.speculated_float_placement_out.right > Au(0)
    }

    /// Compute the fragment position relative to the parent stacking context. If the fragment
    /// itself establishes a stacking context, then the origin of its position will be (0, 0)
    /// for the purposes of this computation.
    pub fn stacking_relative_border_box_for_display_list(&self, fragment: &Fragment) -> Rect<Au> {
        fragment.stacking_relative_border_box(
            &self.stacking_relative_position,
            &self
                .early_absolute_position_info
                .relative_containing_block_size,
            self.early_absolute_position_info
                .relative_containing_block_mode,
            CoordinateSystem::Own,
        )
    }
}

impl ImmutableFlowUtils for &dyn Flow {
    /// Returns true if this flow is a block flow or subclass thereof.
    fn is_block_like(self) -> bool {
        self.class().is_block_like()
    }

    /// Returns true if this flow is a table row flow.
    fn is_table_row(self) -> bool {
        matches!(self.class(), FlowClass::TableRow)
    }

    /// Returns true if this flow is a table cell flow.
    fn is_table_cell(self) -> bool {
        matches!(self.class(), FlowClass::TableCell)
    }

    /// Returns true if this flow is a table colgroup flow.
    fn is_table_colgroup(self) -> bool {
        matches!(self.class(), FlowClass::TableColGroup)
    }

    /// Returns true if this flow is a table flow.
    fn is_table(self) -> bool {
        matches!(self.class(), FlowClass::Table)
    }

    /// Returns true if this flow is a table caption flow.
    fn is_table_caption(self) -> bool {
        matches!(self.class(), FlowClass::TableCaption)
    }

    /// Returns true if this flow is a table rowgroup flow.
    fn is_table_rowgroup(self) -> bool {
        matches!(self.class(), FlowClass::TableRowGroup)
    }

    /// Returns the number of children that this flow possesses.
    fn child_count(self) -> usize {
        self.base().children.len()
    }

    /// Returns true if this flow is a block flow.
    fn is_block_flow(self) -> bool {
        matches!(self.class(), FlowClass::Block)
    }

    /// Returns true if this flow is an inline flow.
    fn is_inline_flow(self) -> bool {
        matches!(self.class(), FlowClass::Inline)
    }

    /// Dumps the flow tree for debugging.
    fn print(self, title: String) {
        let mut print_tree = PrintTree::new(title);
        self.print_with_tree(&mut print_tree);
    }

    /// Dumps the flow tree for debugging into the given PrintTree.
    fn print_with_tree(self, print_tree: &mut PrintTree) {
        print_tree.new_level(format!("{:?}", self));
        self.print_extra_flow_children(print_tree);
        for kid in self.base().child_iter() {
            kid.print_with_tree(print_tree);
        }
        print_tree.end_level();
    }

    fn floats_might_flow_through(self) -> bool {
        if !self.base().might_have_floats_in() && !self.base().might_have_floats_out() {
            return false;
        }
        if self.is_root() {
            return false;
        }
        if !self.is_block_like() {
            return true;
        }
        self.as_block().formatting_context_type() == FormattingContextType::None
    }

    fn baseline_offset_of_last_line_box_in_flow(self) -> Option<Au> {
        for kid in self.base().children.iter().rev() {
            if kid.is_inline_flow() {
                if let Some(baseline_offset) = kid.as_inline().baseline_offset_of_last_line() {
                    return Some(kid.base().position.start.b + baseline_offset);
                }
            }
            if kid.is_block_like() &&
                !kid.base()
                    .flags
                    .contains(FlowFlags::IS_ABSOLUTELY_POSITIONED)
            {
                if let Some(baseline_offset) = kid.baseline_offset_of_last_line_box_in_flow() {
                    return Some(kid.base().position.start.b + baseline_offset);
                }
            }
        }
        None
    }
}

impl MutableFlowUtils for &mut dyn Flow {
    /// Calls `repair_style` and `bubble_inline_sizes`. You should use this method instead of
    /// calling them individually, since there is no reason not to perform both operations.
    fn repair_style_and_bubble_inline_sizes(self, style: &crate::ServoArc<ComputedValues>) {
        self.repair_style(style);
        self.mut_base().update_flags_if_needed(style);
        self.bubble_inline_sizes();
    }
}

impl MutableOwnedFlowUtils for FlowRef {
    /// Set absolute descendants for this flow.
    ///
    /// Set yourself as the Containing Block for all the absolute descendants.
    ///
    /// This is called during flow construction, so nothing else can be accessing the descendant
    /// flows. This is enforced by the fact that we have a mutable `FlowRef`, which only flow
    /// construction is allowed to possess.
    fn set_absolute_descendants(&mut self, abs_descendants: AbsoluteDescendants) {
        let this = self.clone();
        let base = FlowRef::deref_mut(self).mut_base();
        base.abs_descendants = abs_descendants;
        for descendant_link in base.abs_descendants.descendant_links.iter_mut() {
            debug_assert!(!descendant_link.has_reached_containing_block);
            let descendant_base = FlowRef::deref_mut(&mut descendant_link.flow).mut_base();
            descendant_base.absolute_cb.set(this.clone());
        }
    }

    /// Push absolute descendants for this flow.
    ///
    /// Set yourself as the Containing Block for the provided absolute descendants.
    ///
    /// This is called when retreiving layout root if it's not absolute positioned. We can't just
    /// call `set_absolute_descendants` because it might contain other abs_descendants already.
    /// We push descendants instead of replace it since it won't cause circular reference.
    fn push_absolute_descendants(&mut self, mut abs_descendants: AbsoluteDescendants) {
        let this = self.clone();
        let base = FlowRef::deref_mut(self).mut_base();

        for descendant_link in abs_descendants.descendant_links.iter_mut() {
            // TODO(servo#30573) revert to debug_assert!() once underlying bug is fixed
            #[cfg(debug_assertions)]
            if descendant_link.has_reached_containing_block {
                log::warn!("debug assertion failed! !descendant_link.has_reached_containing_block");
            }
            let descendant_base = FlowRef::deref_mut(&mut descendant_link.flow).mut_base();
            descendant_base.absolute_cb.set(this.clone());
        }

        base.abs_descendants.push_descendants(abs_descendants);
    }

    /// Sets the flow as the containing block for all absolute descendants that have been marked
    /// as having reached their containing block. This is needed in order to handle cases like:
    ///
    /// ```html
    /// <div>
    ///     <span style="position: relative">
    ///         <span style="position: absolute; ..."></span>
    ///     </span>
    /// </div>
    /// ```
    fn take_applicable_absolute_descendants(
        &mut self,
        absolute_descendants: &mut AbsoluteDescendants,
    ) {
        let mut applicable_absolute_descendants = AbsoluteDescendants::new();
        for absolute_descendant in absolute_descendants.descendant_links.iter() {
            if absolute_descendant.has_reached_containing_block {
                applicable_absolute_descendants.push(absolute_descendant.flow.clone());
            }
        }
        absolute_descendants
            .descendant_links
            .retain(|descendant| !descendant.has_reached_containing_block);

        let this = self.clone();
        let base = FlowRef::deref_mut(self).mut_base();
        base.abs_descendants = applicable_absolute_descendants;
        for descendant_link in base.abs_descendants.iter() {
            let descendant_base = descendant_link.mut_base();
            descendant_base.absolute_cb.set(this.clone());
        }
    }
}

/// A link to a flow's containing block.
///
/// This cannot safely be a `Flow` pointer because this is a pointer *up* the tree, not *down* the
/// tree. A pointer up the tree is unsafe during layout because it can be used to access a node
/// with an immutable reference while that same node is being laid out, causing possible iterator
/// invalidation and use-after-free.
///
/// FIXME(pcwalton): I think this would be better with a borrow flag instead of `unsafe`.
#[derive(Clone)]
pub struct ContainingBlockLink {
    /// The pointer up to the containing block.
    link: Option<WeakFlowRef>,
}

impl ContainingBlockLink {
    fn new() -> ContainingBlockLink {
        ContainingBlockLink { link: None }
    }

    fn set(&mut self, link: FlowRef) {
        self.link = Some(FlowRef::downgrade(&link))
    }

    #[inline]
    pub fn generated_containing_block_size(&self, for_flow: OpaqueFlow) -> LogicalSize<Au> {
        match self.link {
            None => panic!(
                "Link to containing block not established; perhaps you forgot to call \
                 `set_absolute_descendants`?"
            ),
            Some(ref link) => {
                let flow = link.upgrade().unwrap();
                flow.generated_containing_block_size(for_flow)
            },
        }
    }

    #[inline]
    pub fn explicit_block_containing_size(
        &self,
        shared_context: &SharedStyleContext,
    ) -> Option<Au> {
        match self.link {
            None => panic!(
                "Link to containing block not established; perhaps you forgot to call \
                 `set_absolute_descendants`?"
            ),
            Some(ref link) => {
                let flow = link.upgrade().unwrap();
                if flow.is_block_like() {
                    flow.as_block()
                        .explicit_block_containing_size(shared_context)
                } else if flow.is_inline_flow() {
                    Some(flow.as_inline().minimum_line_metrics.space_above_baseline)
                } else {
                    None
                }
            },
        }
    }
}

/// A wrapper for the pointer address of a flow. These pointer addresses may only be compared for
/// equality with other such pointer addresses, never dereferenced.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OpaqueFlow(pub usize);

impl OpaqueFlow {
    pub fn from_flow(flow: &dyn Flow) -> OpaqueFlow {
        let object_ptr: *const dyn Flow = flow;
        let data_ptr = object_ptr as *const ();
        OpaqueFlow(data_ptr as usize)
    }
}
