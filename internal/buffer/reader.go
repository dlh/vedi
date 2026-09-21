package buffer

import "io"

// Fill reads r into buf until EOF or an error, which it passes to
// buf.Finish. notify runs after every read and after Finish; callers
// should coalesce.
func Fill(r io.Reader, buf *Buffer, notify func()) {
	p := make([]byte, 64*1024)
	nl := false
	for {
		n, err := r.Read(p)
		if n > 0 {
			nl = p[n-1] == '\n'
			buf.Write(p[:n])
			notify()
		}
		if err != nil {
			if err == io.EOF {
				err = nil
			}
			buf.Finish(err, nl)
			notify()
			return
		}
	}
}
