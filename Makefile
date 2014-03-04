PROGRAM_NAME = zhtta
GASH = www/gash

all: $(PROGRAM_NAME) $(GASH)

$(PROGRAM_NAME): $(PROGRAM_NAME).rs
	rustc $(PROGRAM_NAME).rs

$(GASH): $(GASH).rs
	rustc $(GASH).rs

clean :
	$(RM) $(PROGRAM_NAME)
	$(RM) $(GASH)

    
run: ${PROGRAM_NAME}
	./${PROGRAM_NAME}

