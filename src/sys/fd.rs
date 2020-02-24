use std::io::{self, Read, Write, Seek, SeekFrom};
use std::fs::File;
use std::os::unix::io::{FromRawFd, AsRawFd, IntoRawFd, RawFd};
use std::os::unix::fs::FileExt;

#[derive(Debug)]
pub struct FileDesc(File);

impl FileDesc {
    pub unsafe fn new(fd: RawFd) -> Self {
        FileDesc(File::from_raw_fd(fd))
    }

    pub fn try_clone(&self) -> io::Result<FileDesc> {
        Ok(FileDesc(self.0.try_clone()?))
    }
}

impl FromRawFd for FileDesc {
    unsafe fn from_raw_fd(fd: RawFd) -> Self {
        FileDesc(File::from_raw_fd(fd))
    }
}

impl AsRawFd for FileDesc {
    fn as_raw_fd(&self) -> RawFd {
        self.0.as_raw_fd()
    }
}

impl IntoRawFd for FileDesc {
    fn into_raw_fd(self) -> RawFd {
        self.0.into_raw_fd()
    }
}

impl Read for FileDesc {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

impl Read for &FileDesc {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        (&self.0).read(buf)
    }
}

impl Write for FileDesc {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Write for &FileDesc {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        (&self.0).write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Seek for FileDesc {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.0.seek(pos)
    }
}

impl Seek for &FileDesc {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        (&self.0).seek(pos)
    }
}

impl FileExt for FileDesc {
    fn read_at(&self, buf: &mut [u8], offset: u64) -> io::Result<usize> {
        self.0.read_at(buf, offset)
    }
    fn write_at(&self, buf: &[u8], offset: u64) -> io::Result<usize> {
        self.0.write_at(buf, offset)
    }
}
