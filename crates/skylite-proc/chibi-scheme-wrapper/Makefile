include ../chibi-scheme/Makefile.detect

%.o: %.c
	$(CC) -c $(XCPPFLAGS) $(XCFLAGS) $(CLIBFLAGS) -I../chibi-scheme/include -o $@ $<

libwrapper.a: wrapper.o
	$(AR) rcs $@ $^
