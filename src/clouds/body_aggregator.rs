
pub struct AsyncBufList<T> {
    bufs: std::collections::VecDeque<T>,
}

impl<T: bytes::Buf> AsyncBufList<T> {
    pub fn new() -> AsyncBufList<T> {
        AsyncBufList {
            bufs: std::collections::VecDeque::new(),
        }
    }

    #[inline]
    pub fn push(&mut self, buf: T) {
        debug_assert!(buf.has_remaining());
        self.bufs.push_back(buf);
    }

    #[inline]
    #[cfg(feature = "http1")]
    pub fn bufs_cnt(&self) -> usize {
        self.bufs.len()
    }
}

impl<T: bytes::Buf> bytes::Buf for AsyncBufList<T> {
    #[inline]
    fn remaining(&self) -> usize {
        self.bufs.iter().map(|buf| buf.remaining()).sum()
    }

    #[inline]
    fn chunk(&self) -> &[u8] {
        self.bufs.front().map(bytes::Buf::chunk).unwrap_or_default()
    }

    #[inline]
    fn advance(&mut self, mut cnt: usize) {
        while cnt > 0 {
            {
                let front = &mut self.bufs[0];
                let rem = front.remaining();
                if rem > cnt {
                    front.advance(cnt);
                    return;
                } else {
                    front.advance(rem);
                    cnt -= rem;
                }
            }
            self.bufs.pop_front();
        }
    }

    #[inline]
    fn chunks_vectored<'t>(&'t self, dst: &mut [std::io::IoSlice<'t>]) -> usize {
        if dst.is_empty() {
            return 0;
        }
        let mut vecs = 0;
        for buf in &self.bufs {
            vecs += buf.chunks_vectored(&mut dst[vecs..]);
            if vecs == dst.len() {
                break;
            }
        }
        vecs
    }

    #[inline]
    fn copy_to_bytes(&mut self, len: usize) -> bytes::Bytes {
        use bytes::BufMut; // for bm.put
        // Our inner buffer may have an optimized version of copy_to_bytes, and if the whole
        // request can be fulfilled by the front buffer, we can take advantage.
        match self.bufs.front_mut() {
            Some(front) if front.remaining() == len => {
                let b = front.copy_to_bytes(len);
                self.bufs.pop_front();
                b
            }
            Some(front) if front.remaining() > len => front.copy_to_bytes(len),
            _ => {
                assert!(len <= self.remaining(), "`len` greater than remaining");
                let mut bm = bytes::BytesMut::with_capacity(len);
                bm.put(self.take(len));
                bm.freeze()
            }
        }
    }
}

pub async fn async_aggregate<T>(body: T) -> Result<impl bytes::Buf, <T as http_body::Body>::Error>
//    pub async fn async_aggregate<T>(body: T) -> Result<impl Buf, T::Error>
    where
        T: http_body::Body,
{
    use http_body::Body; // for body.data()
    use bytes::Buf; // for buf.has_remaining()

    let mut bufs = AsyncBufList::new();

    futures_util::pin_mut!(body);
    while let Some(buf) = body.data().await {
        let buf = buf?;
        if buf.has_remaining() {
            bufs.push(buf);
        }
    }

    Ok(bufs)
}
