use crate::error::Error;
use crate::token::{Token, Tokenizer};
use std::fmt;

#[derive(PartialEq, Eq, Debug)]
pub enum NoteCommand {
    Summary { title: String },
    Remove { title: String },
}

#[derive(PartialEq, Eq, Debug)]
pub enum ParseError {
    MissingTitle,
}
impl std::error::Error for ParseError {}
impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ParseError::MissingTitle => write!(f, "missing required summary title"),
        }
    }
}

impl NoteCommand {
    pub fn parse<'a>(input: &mut Tokenizer<'a>) -> Result<Option<Self>, Error<'a>> {
        let mut toks = input.clone();
        if let Some(Token::Word("note")) = toks.peek_token()? {
            toks.next_token()?;
            let mut remove = false;
            loop {
                match toks.next_token()? {
                    Some(Token::Word(title)) if title == "remove" => {
                        remove = true;
                        continue;
                    }
                    Some(Token::Quote(title)) => {
                        break Ok(Some(if remove {
                            NoteCommand::Remove {
                                title: title.into_owned(),
                            }
                        } else {
                            NoteCommand::Summary {
                                title: title.into_owned(),
                            }
                        }));
                    }
                    Some(Token::Word(title)) => {
                        break Ok(Some(if remove {
                            NoteCommand::Remove {
                                title: title.to_string(),
                            }
                        } else {
                            NoteCommand::Summary {
                                title: title.to_string(),
                            }
                        }));
                    }
                    _ => break Err(toks.error(ParseError::MissingTitle)),
                };
            }
        } else {
            Ok(None)
        }
    }
}
