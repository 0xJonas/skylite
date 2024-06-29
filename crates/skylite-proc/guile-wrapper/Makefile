CFLAGS = -g -g3 -O3 `pkg-config guile-3.0 --cflags`

%.o: %.c
	$(CC) -c $(CFLAGS) -o $@ $<

libwrapper.a: wrapper.o
	$(AR) rcs $@ $^
