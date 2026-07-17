use std::{
    pin::Pin,
    task::{Context, Poll},
};

use axum::body::{Body, Bytes, HttpBody};
use http_body::Frame;

use super::leases::RouteRequestLease;

pub(super) struct LeasedBody {
    inner: Body,
    lease: Option<RouteRequestLease>,
}

impl LeasedBody {
    pub(super) fn new(inner: Body, lease: RouteRequestLease) -> Self {
        Self {
            inner,
            lease: Some(lease),
        }
    }

    fn release(&mut self) {
        self.lease.take();
    }
}

impl HttpBody for LeasedBody {
    type Data = Bytes;
    type Error = axum::Error;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let this = self.get_mut();
        match Pin::new(&mut this.inner).poll_frame(cx) {
            Poll::Ready(None) => {
                this.release();
                Poll::Ready(None)
            }
            Poll::Ready(Some(Err(error))) => {
                this.release();
                Poll::Ready(Some(Err(error)))
            }
            result => result,
        }
    }

    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }

    fn size_hint(&self) -> http_body::SizeHint {
        self.inner.size_hint()
    }
}
