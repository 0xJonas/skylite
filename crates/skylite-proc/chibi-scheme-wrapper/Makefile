CFLAGS = -DSEXP_USE_NTPGETTIME -DSEXP_USE_INTTYPES -g -g3 -O3

%.o: %.c
	$(CC) -c $(CFLAGS) -o $@ $<

libwrapper.a: wrapper.o
	$(AR) rcs $@ $^
