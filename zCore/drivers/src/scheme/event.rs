//! Event scheme macro.
//!
//! The [`EventScheme`] trait and [`EventHandler`] type alias are defined
//! in the `hal` crate. This module provides the `impl_event_scheme!`
//! macro which generates implementations that delegate to an
//! `EventListener` field.

/// Generate an [`EventScheme`](hal::EventScheme) implementation that delegates
/// to a `self.listener` field of type [`EventListener`](crate::utils::EventListener).
macro_rules! impl_event_scheme {
    ($struct:ident $(, $event_ty:ty)?) => {
        impl_event_scheme!(@impl_base $struct $(, $event_ty)?);
    };
    ($struct:ident<'_> $(, $event_ty:ty)?) => {
        impl_event_scheme!(@impl_base $struct<'_> $(, $event_ty)?);
    };
    ($struct:ident < $($types:ident),* > $(where $($preds:tt)+)? $(, $event_ty:ty)?) => {
        impl_event_scheme!(@impl_base $struct < $($types),* > $(where $($preds)+)? $(, $event_ty)?);
    };

    (@impl_base $struct:ident $(, $event_ty:ty)?) => {
        impl hal::EventScheme for $struct {
            impl_event_scheme!(@impl_body $(, $event_ty)?);
        }
    };
    (@impl_base $struct:ident<'_> $(, $event_ty:ty)?) => {
        impl hal::EventScheme for $struct<'_> {
            impl_event_scheme!(@impl_body $(, $event_ty)?);
        }
    };
    (@impl_base $struct:ident < $($types:ident),* > $(where $($preds:tt)+)? $(, $event_ty:ty)?) => {
        impl < $($types),* > hal::EventScheme for $struct < $($types),* >
            $(where $($preds)+)?
        {
            impl_event_scheme!(@impl_body $(, $event_ty)?);
        }
    };

    (@impl_assoc_type) => {
        type Event = ();
    };
    (@impl_assoc_type, $event_ty:ty) => {
        type Event = $event_ty;
    };
    (@impl_body $(, $event_ty:ty)?) => {
        impl_event_scheme!(@impl_assoc_type $(, $event_ty)?);

        #[inline]
        fn trigger(&self, event: Self::Event) {
            self.listener.trigger(event);
        }

        #[inline]
        fn subscribe(&self, handler: hal::scheme::event::EventHandler<Self::Event>, once: bool) {
            self.listener.subscribe(handler, once);
        }
    };
}
