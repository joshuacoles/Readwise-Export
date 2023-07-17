function metadata({ book, highlights }) {
    return {
        id: book.id,
        updated: book.updated,
        title: book.title,
        author: book.author
    };
}
